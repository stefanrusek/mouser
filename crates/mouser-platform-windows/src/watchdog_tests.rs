use crate::capture::{LOST_HOLD, Problems};
use crate::watchdog::*;
use mouser_platform::{CaptureLossReason, PlatformProblem};
use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

/// A PC whose reasons not to probe the test sets.
struct FakeDesk {
    skip: Option<Skip>,
    probed: u32,
    /// Someone keeps using the PC: its last-input tick moves every time it
    /// is read. Otherwise it moves only when the test says.
    busy: bool,
    tick: Cell<u32>,
    /// Why the capture can no longer see input, if the test says so.
    lost: Option<CaptureLossReason>,
}

impl Surroundings for FakeDesk {
    fn skip(&mut self) -> Option<Skip> {
        self.skip
    }

    fn lost(&mut self) -> Option<CaptureLossReason> {
        self.lost
    }

    fn probed(&mut self) {
        self.probed += 1;
    }

    fn foreground(&self) -> String {
        "game.exe".into()
    }

    fn input_tick(&self) -> u32 {
        if self.busy {
            self.tick.set(self.tick.get() + 1);
        }
        self.tick.get()
    }
}

/// Drives the watchdog's cycles by hand: the hooks have seen `counters`, a
/// probe sent reaches the hooks in `reaches`, and what came out is recorded.
struct Cycles {
    judge: Judge,
    counters: Cell<Counts>,
    /// Which hooks see a probe within its wait.
    reaches: (bool, bool),
    refuse: bool,
    desk: FakeDesk,
    sent: u32,
    reinstalls: Vec<Vec<Hook>>,
    heard: Arc<Mutex<Vec<PlatformProblem>>>,
    sink: Problems,
    notes: RefCell<Vec<Fields>>,
    /// Why the capture was taken away, each time.
    lost: RefCell<Vec<CaptureLossReason>>,
}

impl Cycles {
    fn new() -> Self {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sunk = heard.clone();
        Cycles {
            judge: Judge::default(),
            counters: Cell::new((0, 0)),
            reaches: (true, true),
            refuse: false,
            desk: FakeDesk {
                skip: None,
                probed: 0,
                busy: true,
                tick: Cell::new(0),
                lost: None,
            },
            lost: RefCell::new(Vec::new()),
            sent: 0,
            reinstalls: Vec::new(),
            heard,
            sink: Arc::new(move |p| sunk.lock().unwrap().push(p)),
            notes: RefCell::new(Vec::new()),
        }
    }

    /// One cycle. Returns whether a probe was sent.
    fn run(&mut self) -> bool {
        let sent = Cell::new(false);
        let (counters, reaches, refuse) = (&self.counters, self.reaches, self.refuse);
        let reinstalls = &mut self.reinstalls;
        watch_cycle(
            &mut self.judge,
            Cycle {
                counters: &|| counters.get(),
                probe: &mut || {
                    if refuse {
                        return Outcome::NotSent;
                    }
                    sent.set(true);
                    let (m, k) = counters.get();
                    counters.set((m + u32::from(reaches.0), k + u32::from(reaches.1)));
                    Outcome::Sent
                },
                reinstall: &mut |hooks| reinstalls.push(hooks.to_vec()),
                surroundings: &mut self.desk,
                problems: &self.sink,
                note: &|fields| self.notes.borrow_mut().push(fields),
                lost: &|reason| self.lost.borrow_mut().push(reason),
            },
        );
        self.sent += u32::from(sent.get());
        sent.get()
    }

    /// Someone presses a key or moves the mouse; whether the hooks see it is
    /// up to `arrive`.
    fn someone_used_the_pc(&self) {
        self.desk.tick.set(self.desk.tick.get() + 1);
    }

    /// The hooks see events: probes that were late, or the person's input.
    fn arrive(&self, mouse: bool, key: bool) {
        let (m, k) = self.counters.get();
        self.counters
            .set((m + u32::from(mouse), k + u32::from(key)));
    }

    fn heard(&self) -> Vec<PlatformProblem> {
        std::mem::take(&mut self.heard.lock().unwrap())
    }

    /// The last log line, as `key=value` words.
    fn last_note(&self) -> String {
        let notes = self.notes.borrow();
        let fields = notes.last().expect("a line was logged");
        fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn raised() -> PlatformProblem {
    PlatformProblem::Raised(LOST_HOLD.into())
}

fn cleared() -> PlatformProblem {
    PlatformProblem::Cleared(LOST_HOLD.into())
}

#[test]
fn healthy_hooks_probe_every_cycle_and_say_nothing() {
    let mut c = Cycles::new();
    for _ in 0..3 {
        assert!(c.run());
    }
    assert!(c.reinstalls.is_empty());
    assert!(c.heard().is_empty());
    assert_eq!(c.desk.probed, 3);
    assert_eq!(c.last_note(), "outcome=seen foreground=game.exe");
}

#[test]
fn a_probe_that_is_only_late_is_not_a_miss() {
    let mut c = Cycles::new();
    c.reaches = (false, false);
    assert!(c.run());
    assert_eq!(
        c.last_note(),
        "outcome=unseen hook=mouse,keyboard foreground=game.exe"
    );
    // The guest stalled: the wait passed, and the hooks saw the probe afterwards.
    c.arrive(true, true);
    assert!(c.run());
    assert!(c.reinstalls.is_empty());
    assert!(c.heard().is_empty());
    assert!(
        c.notes
            .borrow()
            .iter()
            .any(|n| n.contains(&("outcome", "late".into())))
    );
}

#[test]
fn a_hook_that_saw_nothing_while_nobody_used_the_pc_has_not_missed() {
    let mut c = Cycles::new();
    c.desk.busy = false;
    c.reaches = (false, false);
    assert!(c.run());
    // Nobody touched the PC since, so the hooks' silence says nothing.
    for _ in 0..3 {
        c.run();
    }
    assert!(c.reinstalls.is_empty());
    assert!(c.heard().is_empty());
    // Someone types, and the hooks still see nothing: that is a miss.
    c.someone_used_the_pc();
    assert!(!c.run());
    assert_eq!(c.reinstalls, [vec![Hook::Mouse, Hook::Keyboard]]);
}

#[test]
fn probes_another_program_swallows_are_no_miss_while_real_input_arrives() {
    // Another program's hook (AsHotplugCtrl) eats every probe before
    // Mouser's sees it, while the person's own moves and keys get through.
    let mut c = Cycles::new();
    c.reaches = (false, false);
    for _ in 0..5 {
        assert!(c.run());
        c.someone_used_the_pc();
        c.arrive(true, true);
    }
    assert!(c.reinstalls.is_empty());
    assert!(c.heard().is_empty());
}

#[test]
fn repeated_stalls_that_always_arrive_late_never_raise_anything() {
    let mut c = Cycles::new();
    c.reaches = (false, false);
    for _ in 0..5 {
        assert!(c.run());
        c.arrive(true, true);
    }
    assert!(c.reinstalls.is_empty());
    assert!(c.heard().is_empty());
}

#[test]
fn one_miss_reinstalls_only_the_hook_that_missed_and_tells_nobody() {
    for (reaches, missed) in [
        ((true, false), Hook::Keyboard),
        ((false, true), Hook::Mouse),
    ] {
        let mut c = Cycles::new();
        c.reaches = reaches;
        assert!(c.run());
        // Still unseen at the next cycle: reinstall that hook, and let it settle.
        assert!(!c.run());
        assert_eq!(c.reinstalls, [vec![missed]]);
        assert_eq!(
            c.last_note(),
            format!(
                "outcome=missed hook={} misses=1 foreground=game.exe",
                missed.name()
            )
        );
        // The reinstall cured it.
        c.reaches = (true, true);
        assert!(c.run());
        assert!(c.run());
        assert_eq!(c.reinstalls.len(), 1);
        assert!(c.heard().is_empty(), "a single miss is not shown");
    }
}

#[test]
fn one_real_miss_takes_the_capture_away() {
    let mut c = Cycles::new();
    c.reaches = (true, false);
    c.run();
    assert!(c.lost.borrow().is_empty(), "unseen is not missed yet");
    c.run();
    assert_eq!(*c.lost.borrow(), [CaptureLossReason::HooksSilent]);
}

#[test]
fn what_the_pc_says_each_cycle_can_take_the_capture_away() {
    for reason in [
        CaptureLossReason::ElevatedForeground,
        CaptureLossReason::OtherDesktop,
    ] {
        let mut c = Cycles::new();
        c.desk.lost = Some(reason);
        c.run();
        assert_eq!(*c.lost.borrow(), [reason]);
    }
    let mut c = Cycles::new();
    c.run();
    assert!(c.lost.borrow().is_empty(), "nothing wrong");
}

#[test]
fn two_consecutive_misses_of_a_hook_tell_the_owner_once() {
    let mut c = Cycles::new();
    c.reaches = (true, false);
    c.run();
    c.run(); // miss 1: reinstalled
    assert!(c.heard().is_empty());
    assert!(c.run()); // the reinstalled hook misses too
    assert!(!c.run()); // miss 2
    assert_eq!(c.heard(), [raised()]);
    assert_eq!(c.reinstalls, [vec![Hook::Keyboard], vec![Hook::Keyboard]]);
    // It goes on: reinstalled again, not raised again.
    c.run();
    c.run();
    assert_eq!(c.reinstalls.len(), 3);
    assert!(c.heard().is_empty());
}

#[test]
fn misses_of_different_hooks_are_not_consecutive() {
    let mut c = Cycles::new();
    c.reaches = (true, false);
    c.run();
    c.run(); // keyboard missed once
    c.reaches = (false, true);
    c.run();
    c.run(); // mouse missed once; the keyboard saw this probe
    assert_eq!(c.reinstalls, [vec![Hook::Keyboard], vec![Hook::Mouse]]);
    assert!(c.heard().is_empty());
}

#[test]
fn both_hooks_missing_are_reinstalled_together() {
    let mut c = Cycles::new();
    c.reaches = (false, false);
    c.run();
    c.run();
    assert_eq!(c.reinstalls, [vec![Hook::Mouse, Hook::Keyboard]]);
}

#[test]
fn the_alarm_clears_once_every_hook_sees_a_probe_again() {
    let mut c = Cycles::new();
    c.reaches = (false, false);
    for _ in 0..4 {
        c.run();
    }
    assert_eq!(c.heard(), [raised()]);
    c.reaches = (true, false);
    c.run();
    assert!(c.heard().is_empty(), "the keyboard is still missing");
    c.run(); // keyboard miss, reinstalled
    c.reaches = (true, true);
    assert!(c.run());
    assert_eq!(c.heard(), [cleared()]);
    assert!(c.run());
    assert!(c.heard().is_empty());
}

#[test]
fn a_late_probe_after_a_raise_clears_the_alarm() {
    let mut c = Cycles::new();
    c.reaches = (false, false);
    for _ in 0..4 {
        c.run();
    }
    assert_eq!(c.heard(), [raised()]);
    c.run(); // the next probe, unseen in its wait
    c.arrive(true, true);
    c.desk.skip = Some(Skip::Idle);
    assert!(!c.run());
    assert_eq!(c.heard(), [cleared()]);
}

#[test]
fn a_probe_the_system_would_not_take_says_nothing_about_the_hooks() {
    let mut c = Cycles::new();
    c.refuse = true;
    assert!(!c.run());
    assert!(!c.run());
    assert!(c.reinstalls.is_empty());
    assert!(c.heard().is_empty());
    assert_eq!(c.desk.probed, 0);
    assert_eq!(c.last_note(), "outcome=not_sent foreground=game.exe");
}

#[test]
fn no_probe_is_sent_while_there_is_a_reason_not_to() {
    for why in [Skip::Locked, Skip::OtherDesktop, Skip::Elevated, Skip::Idle] {
        let mut c = Cycles::new();
        c.desk.skip = Some(why);
        assert!(!c.run());
        assert_eq!(c.sent, 0);
        assert!(
            c.last_note().starts_with("outcome=skipped reason="),
            "{why:?}"
        );
    }
    let mut c = Cycles::new();
    c.desk.skip = Some(Skip::Elevated);
    c.run();
    assert_eq!(
        c.last_note(),
        "outcome=skipped reason=elevated_foreground foreground=game.exe"
    );
}

#[test]
fn a_probe_eaten_before_a_skip_is_still_judged() {
    let mut c = Cycles::new();
    c.reaches = (false, true);
    c.run();
    c.desk.skip = Some(Skip::Locked);
    assert!(!c.run());
    assert_eq!(c.reinstalls, [vec![Hook::Mouse]]);
    // Skipped cycles neither count as misses nor cure them.
    for _ in 0..5 {
        c.run();
    }
    assert_eq!(c.reinstalls.len(), 1);
    assert!(c.heard().is_empty());
}

#[test]
fn hooks_and_their_bits_round_trip() {
    assert_eq!(bits(&[]), 0);
    assert_eq!(bits(&[Hook::Mouse]), 1);
    assert_eq!(bits(&Hook::BOTH), 3);
    assert_eq!(hooks_in(2).collect::<Vec<_>>(), [Hook::Keyboard]);
    assert_eq!(hooks_in(3).collect::<Vec<_>>(), Hook::BOTH);
    assert_eq!(hooks_in(0).count(), 0);
}

#[test]
fn the_first_reason_that_holds_wins() {
    assert_eq!(
        first_reason(&[(Skip::Locked, false), (Skip::Idle, false)]),
        None
    );
    assert_eq!(
        first_reason(&[
            (Skip::Locked, false),
            (Skip::Elevated, true),
            (Skip::Idle, true)
        ]),
        Some(Skip::Elevated)
    );
}

#[test]
fn the_pc_is_idle_when_nothing_came_in_since_the_last_probe() {
    assert!(!idle(None, 100), "never probed");
    assert!(idle(Some(100), 100));
    assert!(!idle(Some(100), 250), "someone used it");
}

#[test]
fn the_probe_lands_on_the_pixel_the_cursor_is_on() {
    let desk = (0, 0, 2560, 1440);
    assert_eq!(absolute((0, 0), desk), (0, 0));
    for x in [1, 1279, 2559] {
        let (n, _) = absolute((x, 0), desk);
        // Windows maps an absolute coordinate to the pixel n * width / 65536.
        assert_eq!(i64::from(n) * 2560 / 65536, i64::from(x), "{x}");
    }
    // Virtual desktops can start left of and above the primary display.
    let wide = (-1920, -200, 4480, 1640);
    let (nx, ny) = absolute((-1920, -200), wide);
    assert_eq!((nx, ny), (0, 0));
    let (nx, _) = absolute((2559, 0), wide);
    assert_eq!(i64::from(nx) * 4480 / 65536 - 1920, 2559);
    assert_eq!(absolute((9999, 9999), desk), (65535, 65535), "clamped");
    assert_eq!(absolute((5, 5), (0, 0, 0, 0)), (65535, 65535), "no desktop");
}

#[test]
fn this_process_has_a_name_and_the_idle_process_none() {
    let pid = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
    let name = process_name(pid).unwrap();
    assert!(name.ends_with(".exe") && !name.contains('\\'), "{name}");
    assert_eq!(process_name(0), None);
    // Whatever is in front of this test, if anything.
    let _ = foreground_process();
}

#[test]
fn the_real_desk_answers_without_error() {
    // Whatever this machine's state; the answers are not knowable here.
    let _ = session_locked();
    let _ = other_desktop();
    let _ = last_input();
    let mut desk = Desk::default();
    let _ = desk.skip();
    desk.probed();
    let _ = desk.skip();
    let _ = desk.foreground();
    let _ = desk.input_tick();
    let _ = desk.lost();
}

#[test]
fn the_hook_thread_can_be_hurried_and_the_process_unthrottled() {
    assert!(std::thread::spawn(hurry_this_thread).join().unwrap());
    assert!(never_throttle());
}

#[test]
fn desktops_are_told_apart_by_name() {
    use windows::Win32::System::StationsAndDesktops::{
        DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, GetThreadDesktop, OpenDesktopW,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    let own = unsafe { GetThreadDesktop(GetCurrentThreadId()) }.unwrap();
    let name = desktop_name(own);
    assert!(!name.is_empty());
    let named = unsafe {
        OpenDesktopW(
            windows::core::w!("Default"),
            DESKTOP_CONTROL_FLAGS(0),
            false,
            DESKTOP_READOBJECTS.0,
        )
    }
    .unwrap();
    assert!(default_desktop(named));
}
