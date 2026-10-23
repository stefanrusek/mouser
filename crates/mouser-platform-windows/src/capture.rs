//! `InputCapture` with `WH_MOUSE_LL` / `WH_KEYBOARD_LL` (W1, W2).
//!
//! The hooks live on one dedicated thread that does nothing but pump
//! messages. A hook callback only looks at thread-local state and pushes onto
//! a channel (an allocation and a lock-free enqueue, no I/O, never a wait),
//! because Windows lets events through unseen when a callback is slow
//! (probe 1, FINDINGS.md). A second thread turns those raw events into
//! `LocalInput` in desktop logical coordinates.
//!
//! While grabbed the hooks swallow every event. The cursor is parked with
//! `SetCursorPos` (which does not reach the hooks) and, because swallowed
//! moves leave it there and the hook's point is not clamped to the screen
//! (probe 1, `edge`), a motion's delta is its hook point minus the park point.
//! Events carrying Mouser's tag are Mouser's own and are ignored.
//!
//! A watchdog sends a tagged probe event through both hooks every few
//! seconds while the PC is in use; a hook that stopped delivering is
//! installed again, and a Problem is reported if that does not cure it
//! (`watchdog`). The hook thread runs time-critical and the process opts out
//! of power throttling, so a busy PC does not make the hooks time out.

use crate::displays::{self, DisplayCache};
use crate::inject::{MOUSER_TAG, WHEEL_UNITS_PER_PIXEL, key_input, mouse_input, os, send};
use crate::watchdog::{self, Counts, Cycle, Desk, Hook, Judge, Outcome, Surroundings};
use crate::{diag, dpi, keymap};
use mouser_platform::{Error, InputCapture, LocalDisplay, LocalInput, PlatformProblem, Result};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE, MOUSEEVENTF_MOVE_NOCOALESCE, MOUSEEVENTF_VIRTUALDESK,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, GetMessageW, GetSystemMetrics, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_UP, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetCursorPos,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

/// "MOUP": `dwExtraInfo` of the watchdog's probe events.
const PROBE_TAG: usize = 0x4D4F_5550;
/// F24, which nothing binds: the key the watchdog's probe releases.
const PROBE_SCAN: u16 = 0x76;

/// Thread messages for the hook thread. `wParam` of the first and the last is
/// the set of hooks (`watchdog::bits`).
const WM_REINSTALL: u32 = WM_APP + 1;
const WM_STOP: u32 = WM_APP + 2;
/// Removes hooks without installing them again: what the OS does to a hook
/// that timed out. Posted by tests.
const WM_UNHOOK: u32 = WM_APP + 3;

/// The default time between watchdog probes, and how long a probe may take.
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);
const PROBE_WAIT: Duration = Duration::from_millis(500);

/// Reports a Problem to whoever listens (the app shows it).
pub(crate) type Problems = Arc<dyn Fn(PlatformProblem) + Send + Sync>;

/// Input as the hooks see it, in physical pixels and wheel units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Raw {
    Motion { dx: i32, dy: i32, x: i32, y: i32 },
    Button { button: u8, down: bool },
    Scroll { dx: i32, dy: i32 },
    Key { usage: u32, down: bool },
}

/// The mouse message's meaning. `parked` is where the cursor is held while
/// grabbed; `last` the previous hook point while not.
pub(crate) fn mouse_raw(
    msg: u32,
    at: (i32, i32),
    data: u32,
    parked: Option<(i32, i32)>,
    last: &mut Option<(i32, i32)>,
) -> Option<Raw> {
    let button = |button, down| Some(Raw::Button { button, down });
    let wheel = (data >> 16) as u16 as i16 as i32;
    match msg {
        WM_MOUSEMOVE => {
            let from = parked.or(*last).unwrap_or(at);
            *last = if parked.is_some() { None } else { Some(at) };
            let (dx, dy) = (at.0 - from.0, at.1 - from.1);
            if parked.is_some() && (dx, dy) == (0, 0) {
                return None;
            }
            let (x, y) = parked.unwrap_or(at);
            Some(Raw::Motion { dx, dy, x, y })
        }
        WM_LBUTTONDOWN => button(1, true),
        WM_LBUTTONUP => button(1, false),
        WM_RBUTTONDOWN => button(2, true),
        WM_RBUTTONUP => button(2, false),
        WM_MBUTTONDOWN => button(3, true),
        WM_MBUTTONUP => button(3, false),
        WM_XBUTTONDOWN | WM_XBUTTONUP => match data >> 16 {
            x @ (1 | 2) => button(3 + x as u8, msg == WM_XBUTTONDOWN),
            _ => None,
        },
        WM_MOUSEWHEEL => Some(Raw::Scroll { dx: 0, dy: wheel }),
        WM_MOUSEHWHEEL => Some(Raw::Scroll { dx: wheel, dy: 0 }),
        _ => None,
    }
}

/// The key event for a keyboard hook's scan code and flags; `None` for keys
/// Mouser does not forward.
pub(crate) fn key_raw(scan: u32, flags: u32) -> Option<Raw> {
    keymap::usage_for_scan(scan as u16, flags & LLKHF_EXTENDED.0 != 0).map(|usage| Raw::Key {
        usage,
        down: flags & LLKHF_UP.0 == 0,
    })
}

/// `Raw` as `LocalInput` in desktop logical coordinates.
pub(crate) fn convert(all: &[LocalDisplay], raw: Raw) -> LocalInput {
    match raw {
        Raw::Motion { dx, dy, x, y } => {
            let (abs, scale) = displays::pixel_to_logical(all, x, y);
            LocalInput::Motion {
                dx: f64::from(dx) / scale,
                dy: f64::from(dy) / scale,
                abs,
            }
        }
        Raw::Button { button, down } => LocalInput::Button { button, down },
        Raw::Scroll { dx, dy } => LocalInput::Scroll {
            dx: f64::from(dx) / WHEEL_UNITS_PER_PIXEL,
            dy: f64::from(dy) / WHEEL_UNITS_PER_PIXEL,
        },
        Raw::Key { usage, down } => LocalInput::Key { usage, down },
    }
}

fn pack(x: i32, y: i32) -> u64 {
    (u64::from(x as u32) << 32) | u64::from(y as u32)
}

fn unpack(p: u64) -> (i32, i32) {
    ((p >> 32) as u32 as i32, p as u32 as i32)
}

/// State the hook thread, the caller and the watchdog share.
pub(crate) struct Shared {
    pub(crate) grabbed: AtomicBool,
    pub(crate) park: AtomicU64,
    /// Probe events each hook has seen.
    pub(crate) probed_mouse: AtomicU32,
    pub(crate) probed_key: AtomicU32,
}

impl Shared {
    fn parked(&self) -> Option<(i32, i32)> {
        self.grabbed
            .load(Ordering::SeqCst)
            .then(|| unpack(self.park.load(Ordering::SeqCst)))
    }
}

/// What the hook callbacks work with; they run on the hook thread.
pub(crate) struct HookCtx {
    pub(crate) raw: Sender<Raw>,
    pub(crate) shared: Arc<Shared>,
    pub(crate) last: Option<(i32, i32)>,
}

thread_local! {
    static CTX: RefCell<Option<HookCtx>> = const { RefCell::new(None) };
}

/// Whether to swallow a mouse event.
pub(crate) fn on_mouse(ctx: &mut HookCtx, msg: u32, info: &MSLLHOOKSTRUCT) -> bool {
    match info.dwExtraInfo {
        PROBE_TAG => {
            ctx.shared.probed_mouse.fetch_add(1, Ordering::SeqCst);
            // Swallowed, so a probe never moves the cursor.
            return true;
        }
        MOUSER_TAG => return false,
        _ => {}
    }
    let parked = ctx.shared.parked();
    let at = (info.pt.x, info.pt.y);
    if let Some(raw) = mouse_raw(msg, at, info.mouseData, parked, &mut ctx.last) {
        // A receiver that has gone away just means nobody is listening.
        let _ = ctx.raw.send(raw);
    }
    parked.is_some()
}

/// Whether to swallow a keyboard event.
fn on_key(ctx: &mut HookCtx, info: &KBDLLHOOKSTRUCT) -> bool {
    match info.dwExtraInfo {
        PROBE_TAG => {
            ctx.shared.probed_key.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        MOUSER_TAG => return false,
        _ => {}
    }
    if let Some(raw) = key_raw(info.scanCode, info.flags.0) {
        let _ = ctx.raw.send(raw);
    }
    ctx.shared.grabbed.load(Ordering::SeqCst)
}

pub(crate) unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        let swallow = CTX.with_borrow_mut(|c| on_mouse(c.as_mut().unwrap(), wp.0 as u32, info));
        if swallow {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

pub(crate) unsafe extern "system" fn key_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if CTX.with_borrow_mut(|c| on_key(c.as_mut().unwrap(), info)) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// The system calls that install and remove one low-level hook.
pub(crate) trait HookApi {
    /// The new hook, or `None` when the system refuses it.
    fn install(&self, hook: Hook) -> Option<HHOOK>;
    /// Removes it; `false` when the system had already removed it.
    fn remove(&self, handle: HHOOK) -> bool;
}

/// The real system calls.
pub(crate) struct System;

impl HookApi for System {
    fn install(&self, hook: Hook) -> Option<HHOOK> {
        let hinst = unsafe { GetModuleHandleW(None) }.ok()?;
        let (id, proc_) = match hook {
            Hook::Mouse => (WH_MOUSE_LL, mouse_proc as HookProc),
            Hook::Keyboard => (WH_KEYBOARD_LL, key_proc as HookProc),
        };
        unsafe { SetWindowsHookExW(id, Some(proc_), Some(hinst.into()), 0) }.ok()
    }

    fn remove(&self, handle: HHOOK) -> bool {
        unsafe { UnhookWindowsHookEx(handle) }.is_ok()
    }
}

type HookProc = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;

/// The installed hooks.
pub(crate) struct Hooks<A: HookApi> {
    api: A,
    pub(crate) mouse: Option<HHOOK>,
    pub(crate) keyboard: Option<HHOOK>,
}

impl<A: HookApi> Hooks<A> {
    /// Both hooks installed; one the system refuses stays missing, for the
    /// watchdog to find.
    pub(crate) fn install(api: A) -> Self {
        let mut hooks = Hooks {
            api,
            mouse: None,
            keyboard: None,
        };
        for hook in Hook::BOTH {
            hooks.reinstall(hook);
        }
        hooks
    }

    fn slot(&mut self, hook: Hook) -> &mut Option<HHOOK> {
        match hook {
            Hook::Mouse => &mut self.mouse,
            Hook::Keyboard => &mut self.keyboard,
        }
    }

    /// Installs `hook` afresh and only then removes the old one, so no event
    /// slips past in between (both run on this thread, which handles no
    /// event until this returns). A fresh hook the system refuses leaves the
    /// old one in place; an old one the system already removed counts as
    /// removed. Whether the fresh one is in.
    pub(crate) fn reinstall(&mut self, hook: Hook) -> bool {
        let Some(fresh) = self.api.install(hook) else {
            return false;
        };
        if let Some(old) = self.slot(hook).replace(fresh) {
            self.api.remove(old);
        }
        true
    }

    /// Removes `hook`, if it is in.
    pub(crate) fn remove(&mut self, hook: Hook) {
        if let Some(old) = self.slot(hook).take() {
            self.api.remove(old);
        }
    }
}

/// The hook thread: install, pump, obey thread messages, remove.
fn hook_thread(raw: Sender<Raw>, shared: Arc<Shared>, ready: Sender<u32>) {
    dpi::physical_thread();
    let hurried = watchdog::hurry_this_thread();
    diag::note(
        "capture.hook_thread",
        vec![("time_critical", hurried.to_string())],
    );
    CTX.set(Some(HookCtx {
        raw,
        shared,
        last: None,
    }));
    let mut hooks = Hooks::install(System);
    // Thread messages can only be posted once the thread has a queue.
    let mut msg = MSG::default();
    let _ = unsafe { PeekMessageW(&mut msg, None, WM_APP, WM_APP, PM_NOREMOVE) };
    ready
        .send(unsafe { GetCurrentThreadId() })
        .expect("hook thread ready");
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        match msg.message {
            WM_REINSTALL => {
                for hook in watchdog::hooks_in(msg.wParam.0) {
                    let ok = hooks.reinstall(hook).to_string();
                    diag::note(
                        "capture.reinstall",
                        vec![("hook", hook.name().into()), ("ok", ok)],
                    );
                }
            }
            WM_UNHOOK => watchdog::hooks_in(msg.wParam.0).for_each(|hook| hooks.remove(hook)),
            WM_STOP => break,
            _ => {}
        }
    }
    Hook::BOTH.into_iter().for_each(|hook| hooks.remove(hook));
}

/// The probe events each hook has seen so far.
fn seen(shared: &Shared) -> Counts {
    (
        shared.probed_mouse.load(Ordering::SeqCst),
        shared.probed_key.load(Ordering::SeqCst),
    )
}

/// The virtual desktop `(left, top, width, height)` in physical pixels.
fn virtual_desktop() -> (i32, i32, i32, i32) {
    dpi::physical(|| unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    })
}

/// Sends a move to where the cursor already is (absolute, so pointer
/// acceleration cannot shrink it to nothing, and never coalesced) and an F24
/// release, both tagged as probes, and waits up to `wait` for both hooks to
/// see theirs.
pub(crate) fn probe_hooks(shared: &Shared, wait: Duration) -> Outcome {
    probe_hooks_with(shared, wait, &mut || {
        let (x, y) = watchdog::absolute(cursor_pos()?, virtual_desktop());
        let flags = MOUSEEVENTF_MOVE
            | MOUSEEVENTF_ABSOLUTE
            | MOUSEEVENTF_VIRTUALDESK
            | MOUSEEVENTF_MOVE_NOCOALESCE;
        let mut nudge = mouse_input(flags, 0, PROBE_TAG);
        nudge.Anonymous.mi.dx = x;
        nudge.Anonymous.mi.dy = y;
        send(&[nudge]).and_then(|()| send(&[key_input(PROBE_SCAN, false, false, PROBE_TAG)]))
    })
}

pub(crate) fn probe_hooks_with(
    shared: &Shared,
    wait: Duration,
    send_probe: &mut dyn FnMut() -> Result<()>,
) -> Outcome {
    let before = seen(shared);
    if send_probe().is_err() {
        return Outcome::NotSent;
    }
    let end = Instant::now() + wait;
    while Instant::now() < end {
        let now = seen(shared);
        if now.0 != before.0 && now.1 != before.1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Outcome::Sent
}

/// Said when a hook stays lost after it was installed again; withdrawn once
/// every hook sees a probe again. The app keeps it out of system
/// notifications, by this text.
pub const LOST_HOLD: &str = "Mouser lost its hold on the keyboard and mouse and took it back; \
                         input may have briefly leaked to this PC.";

/// Makes the watchdog's view of the PC: the real one, or a test's.
pub(crate) type MakeSurroundings = fn() -> Box<dyn Surroundings + Send>;

fn real_desk() -> Box<dyn Surroundings + Send> {
    Box::new(Desk::default())
}

fn watchdog_thread(
    shared: Arc<Shared>,
    hook_thread_id: u32,
    interval: Duration,
    stop: Receiver<()>,
    problems: Problems,
    mut surroundings: Box<dyn Surroundings + Send>,
) {
    let mut judge = Judge::default();
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(interval) {
        watchdog::watch_cycle(
            &mut judge,
            Cycle {
                counters: &|| seen(&shared),
                probe: &mut || probe_hooks(&shared, PROBE_WAIT),
                reinstall: &mut |hooks| {
                    let which = WPARAM(watchdog::bits(hooks));
                    // A hook thread that is gone has nothing left to reinstall.
                    let _ = unsafe {
                        PostThreadMessageW(hook_thread_id, WM_REINSTALL, which, LPARAM(0))
                    };
                },
                surroundings: surroundings.as_mut(),
                problems: &problems,
                note: &|fields| diag::note("capture.probe", fields),
            },
        );
    }
}

pub(crate) struct Running {
    pub(crate) shared: Arc<Shared>,
    hook_thread_id: u32,
    displays: DisplayCache,
    hook: JoinHandle<()>,
    forward: JoinHandle<()>,
    watchdog: JoinHandle<()>,
    stop_watchdog: Sender<()>,
}

/// Hides or restores the local pointer; a failure is reported as a problem and
/// never stops the grab, since the parked pointer is still held still.
pub(crate) fn set_pointer_hidden(
    hidden: bool,
    problems: &Problems,
    change: impl FnOnce(bool) -> Result<()>,
) {
    if let Err(e) = change(hidden) {
        problems(PlatformProblem::Raised(format!(
            "Mouser could not change the pointer on this PC: {e}"
        )));
    }
}

fn hide_or_show(hidden: bool) -> Result<()> {
    if hidden {
        crate::cursor::hide()
    } else {
        crate::cursor::show()
    }
}

pub(crate) struct WinCapture {
    problems: Problems,
    interval: Duration,
    surroundings: MakeSurroundings,
    running: Option<Running>,
}

impl WinCapture {
    pub(crate) fn new(problems: Problems) -> Self {
        Self::with_watchdog(problems, WATCHDOG_INTERVAL, real_desk)
    }

    pub(crate) fn with_watchdog(
        problems: Problems,
        interval: Duration,
        surroundings: MakeSurroundings,
    ) -> Self {
        WinCapture {
            problems,
            interval,
            surroundings,
            running: None,
        }
    }

    pub(crate) fn running(&self) -> Result<&Running> {
        self.running
            .as_ref()
            .ok_or_else(|| Error::Os("capture has not been started".into()))
    }
}

fn cursor_pos() -> Result<(i32, i32)> {
    let mut p = POINT::default();
    let got = dpi::physical(|| unsafe { GetCursorPos(&mut p) });
    os("GetCursorPos", got)?;
    Ok((p.x, p.y))
}

fn park_cursor(shared: &Shared, x: i32, y: i32) -> Result<()> {
    let set = dpi::physical(|| unsafe { SetCursorPos(x, y) });
    os("SetCursorPos", set)?;
    shared.park.store(pack(x, y), Ordering::SeqCst);
    Ok(())
}

impl InputCapture for WinCapture {
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()> {
        if self.running.is_some() {
            return Err(Error::Os("capture is already started".into()));
        }
        let shared = Arc::new(Shared {
            grabbed: AtomicBool::new(false),
            park: AtomicU64::new(0),
            probed_mouse: AtomicU32::new(0),
            probed_key: AtomicU32::new(0),
        });
        let displays = DisplayCache::new();
        let (raw_tx, raw_rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let hook = {
            let shared = shared.clone();
            std::thread::spawn(move || hook_thread(raw_tx, shared, ready_tx))
        };
        let hook_thread_id = ready_rx.recv().expect("hook thread");
        let forward = {
            let displays = displays.view();
            std::thread::spawn(move || {
                // Ends when the hook thread is gone and its sender dropped.
                while let Ok(raw) = raw_rx.recv() {
                    // A receiver that has gone away just means nobody is listening.
                    let _ = tx.send(convert(&displays.snapshot(), raw));
                }
            })
        };
        let unthrottled = watchdog::never_throttle();
        diag::note(
            "capture.start",
            vec![("unthrottled", unthrottled.to_string())],
        );
        let (stop_watchdog, stop_rx) = channel();
        let watchdog = {
            let (shared, problems, interval) =
                (shared.clone(), self.problems.clone(), self.interval);
            let surroundings = (self.surroundings)();
            std::thread::spawn(move || {
                watchdog_thread(
                    shared,
                    hook_thread_id,
                    interval,
                    stop_rx,
                    problems,
                    surroundings,
                );
            })
        };
        self.running = Some(Running {
            shared,
            hook_thread_id,
            displays,
            hook,
            forward,
            watchdog,
            stop_watchdog,
        });
        Ok(())
    }

    fn set_grabbed(&mut self, grabbed: bool) -> Result<()> {
        let shared = &self.running()?.shared;
        if grabbed {
            let (x, y) = cursor_pos()?;
            park_cursor(shared, x, y)?;
        }
        set_pointer_hidden(grabbed, &self.problems, hide_or_show);
        shared.grabbed.store(grabbed, Ordering::SeqCst);
        Ok(())
    }

    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        let running = self.running()?;
        let (px, py) = displays::logical_to_pixel(&running.displays.snapshot(), x, y);
        park_cursor(&running.shared, px, py)
    }

    fn stop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        if running.shared.grabbed.load(Ordering::SeqCst) {
            set_pointer_hidden(false, &self.problems, hide_or_show);
        }
        let _ = running.stop_watchdog.send(());
        running.watchdog.join().expect("watchdog thread");
        // A hook thread that is already gone has nothing left to stop.
        let _ =
            unsafe { PostThreadMessageW(running.hook_thread_id, WM_STOP, WPARAM(0), LPARAM(0)) };
        running.hook.join().expect("hook thread");
        running.forward.join().expect("forward thread");
    }
}

impl Drop for WinCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// For tests: what the OS does to a hook that timed out.
#[cfg(test)]
impl WinCapture {
    /// A thread message the hook thread has no use for.
    pub(crate) fn poke(&self) {
        let id = self.running().unwrap().hook_thread_id;
        unsafe { PostThreadMessageW(id, WM_APP + 100, WPARAM(0), LPARAM(0)) }.unwrap();
    }

    pub(crate) fn drop_hooks(&self, hooks: &[Hook]) {
        let id = self.running().unwrap().hook_thread_id;
        let which = WPARAM(watchdog::bits(hooks));
        unsafe { PostThreadMessageW(id, WM_UNHOOK, which, LPARAM(0)) }.unwrap();
    }
}
