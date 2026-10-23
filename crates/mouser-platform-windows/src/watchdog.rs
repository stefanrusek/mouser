//! The capture watchdog's judgement (W1, W2): whether each low-level hook
//! still sees input, judged per hook from tagged probe events.
//!
//! A probe is real input to Windows: it resets the idle timer that turns the
//! display off and locks the session. So a probe is sent only when it can
//! tell something and costs nothing: never while nobody has used the PC since
//! the last probe (a probe then follows real input by at most one interval,
//! so it never keeps an idle PC awake), never while the session is locked or
//! another desktop (UAC, Ctrl-Alt-Del) has the input, and never while the
//! foreground window outranks Mouser (UIPI drops what Mouser sends to it,
//! without an error).
//!
//! A probe that is not seen in time is not yet a miss: a stalled guest
//! delivers late, not never. It is a miss if it is still unseen when the next
//! cycle begins, measured in probes (the hook's counter has not moved), never
//! in wall time. A missed hook, and only that one, is installed again. The
//! owner is told only after two consecutive misses of the same hook (a
//! single miss that a reinstall cures is logged, not shown); the problem is
//! withdrawn once every hook sees a probe again.

use crate::capture::{LOST_HOLD, Problems};
use mouser_platform::PlatformProblem;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK, WTSFreeMemory,
    WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, GetUserObjectInformationW, HDESK,
    OpenInputDesktop, UOI_NAME,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE, PROCESS_QUERY_LIMITED_INFORMATION, ProcessPowerThrottling,
    QueryFullProcessImageNameW, SetProcessInformation, SetThreadPriority,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::PWSTR;

/// One of the two low-level hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hook {
    Mouse,
    Keyboard,
}

impl Hook {
    pub(crate) const BOTH: [Hook; 2] = [Hook::Mouse, Hook::Keyboard];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Hook::Mouse => "mouse",
            Hook::Keyboard => "keyboard",
        }
    }

    /// This hook's bit in a thread message's set of hooks.
    pub(crate) fn bit(self) -> usize {
        match self {
            Hook::Mouse => 1,
            Hook::Keyboard => 2,
        }
    }
}

/// `hooks` as a set of bits.
pub(crate) fn bits(hooks: &[Hook]) -> usize {
    hooks.iter().fold(0, |all, hook| all | hook.bit())
}

/// The hooks in the set of bits `bits`.
pub(crate) fn hooks_in(bits: usize) -> impl Iterator<Item = Hook> {
    Hook::BOTH
        .into_iter()
        .filter(move |hook| bits & hook.bit() != 0)
}

/// Probe events each hook has seen: (mouse, keyboard).
pub(crate) type Counts = (u32, u32);

fn count(counts: Counts, hook: Hook) -> u32 {
    match hook {
        Hook::Mouse => counts.0,
        Hook::Keyboard => counts.1,
    }
}

/// Why a probe now would tell nothing, so none is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Skip {
    /// The session is locked.
    Locked,
    /// Another desktop (a UAC prompt, the Ctrl-Alt-Del screen) has the input.
    OtherDesktop,
    /// The foreground window outranks Mouser; UIPI drops what Mouser sends.
    Elevated,
    /// Nobody has used the PC since the last probe.
    Idle,
}

impl Skip {
    fn name(self) -> &'static str {
        match self {
            Skip::Locked => "locked",
            Skip::OtherDesktop => "other_desktop",
            Skip::Elevated => "elevated_foreground",
            Skip::Idle => "idle",
        }
    }
}

/// What the watchdog needs to know about the PC around it.
pub(crate) trait Surroundings {
    /// Why not to probe now, if there is a reason.
    fn skip(&mut self) -> Option<Skip>;
    /// A probe was just sent.
    fn probed(&mut self);
    /// The foreground window's program, for the log.
    fn foreground(&self) -> String;
}

/// Whether a probe was handed to the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Sent,
    /// The system refused it, so nothing is known about the hooks.
    NotSent,
}

/// Consecutive misses of one hook before the owner is told.
pub(crate) const MISSES_TO_RAISE: u32 = 2;

#[derive(Default)]
struct HookState {
    /// What the hook had seen when the probe that is still unseen was sent.
    pending: Option<u32>,
    /// Consecutive probes it missed.
    misses: u32,
}

#[derive(Default)]
pub(crate) struct Judge {
    mouse: HookState,
    keyboard: HookState,
    raised: bool,
}

impl Judge {
    fn state(&mut self, hook: Hook) -> &mut HookState {
        match hook {
            Hook::Mouse => &mut self.mouse,
            Hook::Keyboard => &mut self.keyboard,
        }
    }

    /// Withdraws the alarm once no hook is missing probes.
    fn clear_if_healed(&mut self, problems: &Problems) {
        let healed = self.mouse.misses == 0 && self.keyboard.misses == 0;
        if healed && std::mem::take(&mut self.raised) {
            problems(PlatformProblem::Cleared(LOST_HOLD.into()));
        }
    }
}

/// A debug-level log line's fields.
pub(crate) type Fields = Vec<(&'static str, String)>;

/// What one watchdog cycle works with.
pub(crate) struct Cycle<'a> {
    pub(crate) counters: &'a dyn Fn() -> Counts,
    pub(crate) probe: &'a mut dyn FnMut() -> Outcome,
    /// Installs these hooks again (asynchronously).
    pub(crate) reinstall: &'a mut dyn FnMut(&[Hook]),
    pub(crate) surroundings: &'a mut dyn Surroundings,
    pub(crate) problems: &'a Problems,
    /// Writes a debug-level log line.
    pub(crate) note: &'a dyn Fn(Fields),
}

/// One watchdog cycle: judge the probe still unseen from the last cycle and,
/// unless a hook was just reinstalled or there is a reason not to, send the
/// next one.
pub(crate) fn watch_cycle(judge: &mut Judge, cycle: Cycle) {
    let foreground = cycle.surroundings.foreground();
    let note = |outcome: &str, mut fields: Fields| {
        fields.insert(0, ("outcome", outcome.into()));
        fields.push(("foreground", foreground.clone()));
        (cycle.note)(fields);
    };
    let now = (cycle.counters)();
    let mut failed = Vec::new();
    for hook in Hook::BOTH {
        let state = judge.state(hook);
        let Some(before) = state.pending.take() else {
            continue;
        };
        if count(now, hook) != before {
            state.misses = 0;
            note("late", vec![("hook", hook.name().into())]);
        } else {
            state.misses += 1;
            failed.push(hook);
            let misses = state.misses.to_string();
            note(
                "missed",
                vec![("hook", hook.name().into()), ("misses", misses)],
            );
        }
    }
    if !failed.is_empty() {
        (cycle.reinstall)(&failed);
        let repeated = failed
            .iter()
            .any(|hook| judge.state(*hook).misses >= MISSES_TO_RAISE);
        if repeated && !std::mem::replace(&mut judge.raised, true) {
            (cycle.problems)(PlatformProblem::Raised(LOST_HOLD.into()));
        }
        // The reinstall is asynchronous: probe once it has had a cycle.
        return;
    }
    judge.clear_if_healed(cycle.problems);
    if let Some(why) = cycle.surroundings.skip() {
        note("skipped", vec![("reason", why.name().into())]);
        return;
    }
    if (cycle.probe)() == Outcome::NotSent {
        note("not_sent", vec![]);
        return;
    }
    cycle.surroundings.probed();
    let after = (cycle.counters)();
    let mut unseen = Vec::new();
    for hook in Hook::BOTH {
        let state = judge.state(hook);
        if count(after, hook) != count(now, hook) {
            state.misses = 0;
        } else {
            state.pending = Some(count(now, hook));
            unseen.push(hook.name());
        }
    }
    if unseen.is_empty() {
        note("seen", vec![]);
    } else {
        note("unseen", vec![("hook", unseen.join(","))]);
    }
    judge.clear_if_healed(cycle.problems);
}

/// The first reason in `checks` that holds.
pub(crate) fn first_reason(checks: &[(Skip, bool)]) -> Option<Skip> {
    checks.iter().find(|(_, holds)| *holds).map(|(why, _)| *why)
}

/// Whether nobody used the PC since the last probe: the system's last-input
/// time is still the one read right after it.
pub(crate) fn idle(after_last_probe: Option<u32>, last_input: u32) -> bool {
    after_last_probe == Some(last_input)
}

/// The tick of the last input the system took, from anyone (probes too).
pub(crate) fn last_input() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let _ = unsafe { GetLastInputInfo(&mut info) };
    info.dwTime
}

/// Whether this session is locked; `false` when Windows will not say.
pub(crate) fn session_locked() -> bool {
    let mut buf = PWSTR::null();
    let mut len = 0;
    let got = unsafe {
        WTSQuerySessionInformationW(
            Some(WTS_CURRENT_SERVER_HANDLE),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buf,
            &mut len,
        )
    }
    .is_ok();
    let flags = got.then(|| unsafe {
        let flags = (*buf.0.cast::<WTSINFOEXW>())
            .Data
            .WTSInfoExLevel1
            .SessionFlags;
        WTSFreeMemory(buf.0.cast());
        flags
    });
    flags == Some(WTS_SESSIONSTATE_LOCK as i32)
}

/// The name of desktop `desk` ("Default" for the user's own); empty when
/// Windows will not say (the buffer is then left as it was, all zeros).
pub(crate) fn desktop_name(desk: HDESK) -> String {
    let mut name = [0u16; 64];
    let mut needed = 0;
    let _ = unsafe {
        GetUserObjectInformationW(
            HANDLE(desk.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            size_of_val(&name) as u32,
            Some(&mut needed),
        )
    };
    let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    String::from_utf16_lossy(&name[..len])
}

/// Whether `desk` is the user's own desktop; closes it.
pub(crate) fn default_desktop(desk: HDESK) -> bool {
    let ours = desktop_name(desk) == "Default";
    let _ = unsafe { CloseDesktop(desk) };
    ours
}

/// Whether a desktop other than the user's own has the input; one this
/// process may not even open (the secure desktop) is another.
pub(crate) fn other_desktop() -> bool {
    let opened = unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) };
    !opened.is_ok_and(default_desktop)
}

/// The file name of process `pid`'s program.
pub(crate) fn process_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    let got = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut len,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    got.ok()?;
    let path = String::from_utf16_lossy(&path[..len as usize]);
    path.rsplit('\\').next().map(str::to_string)
}

/// The foreground window's program, empty when there is none or it cannot
/// be read.
pub(crate) fn foreground_process() -> String {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
    process_name(pid).unwrap_or_default()
}

/// The real PC.
#[derive(Default)]
pub(crate) struct Desk {
    /// The system's last-input time read right after the last probe.
    after_last_probe: Option<u32>,
}

impl Surroundings for Desk {
    fn skip(&mut self) -> Option<Skip> {
        // All asked every time: each is cheap, and the watchdog asks every few seconds.
        first_reason(&[
            (Skip::Locked, session_locked()),
            (Skip::OtherDesktop, other_desktop()),
            (Skip::Elevated, crate::uipi::foreground_elevated()),
            (Skip::Idle, idle(self.after_last_probe, last_input())),
        ])
    }

    fn probed(&mut self) {
        self.after_last_probe = Some(last_input());
    }

    fn foreground(&self) -> String {
        foreground_process()
    }
}

/// Runs the calling thread (the hook thread) ahead of everything else, so a
/// busy PC does not make it miss the hook timeout. Whether Windows agreed.
pub(crate) fn hurry_this_thread() -> bool {
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) }.is_ok()
}

/// Opts this process out of power throttling (EcoQoS), which slows a
/// background process's threads, the hook thread among them. Whether
/// Windows agreed (older versions do not know the setting).
pub(crate) fn never_throttle() -> bool {
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            (&raw const state).cast(),
            size_of_val(&state) as u32,
        )
    }
    .is_ok()
}

/// `at` in physical pixels as `SendInput`'s absolute coordinates (0 to
/// 65535) over the virtual desktop `(left, top, width, height)`: the
/// smallest value that lands on that very pixel.
pub(crate) fn absolute(at: (i32, i32), desktop: (i32, i32, i32, i32)) -> (i32, i32) {
    let scale = |v: i32, origin: i32, size: i32| {
        let size = i64::from(size.max(1));
        let n = (i64::from(v - origin) * 65536 + size - 1) / size;
        n.clamp(0, 65535) as i32
    };
    (
        scale(at.0, desktop.0, desktop.2),
        scale(at.1, desktop.1, desktop.3),
    )
}
