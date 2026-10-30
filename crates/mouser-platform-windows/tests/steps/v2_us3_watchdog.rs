//! Steps for `tests/features/v2_us3_watchdog.feature` (Story 3 sc. 16).
//!
//! "Software intercepts Mouser's own checks" is a low-level mouse and
//! keyboard hook installed after Mouser's (so the system calls it first)
//! that swallows every event carrying the watchdog's probe tag, as probe P1
//! `swallow` did for ASUS AsHotplugCtrl. "The mouse and keyboard keep
//! working" is untagged input sent with `SendInput` for several watchdog
//! cycles, seen by Mouser's capture. "Health shows no problem" is that
//! `watch_problems` raises no "lost its hold" problem meanwhile.

use cucumber::{World, given, then, when};
use mouser_platform::{InputCapture, LocalInput, Platform, PlatformProblem, WatchHandle};
use mouser_platform_windows::{LOST_HOLD, WinPlatform};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_QUIT,
};

/// The tag Mouser's watchdog puts on its probes (`capture.rs`, PROBE_TAG).
/// Should it ever differ, nothing is swallowed and the scenario says so.
const PROBE_TAG: usize = 0x4D4F_5550;
/// F23: a key nothing binds, for the desk user's typing.
const F23: u16 = 0x6E;
/// Long enough for four watchdog cycles of 5 s, which would raise the
/// problem twice over if swallowed probes counted as misses.
const CYCLES: Duration = Duration::from_secs(21);

static SWALLOWED_MOUSE: AtomicU32 = AtomicU32::new(0);
static SWALLOWED_KEYS: AtomicU32 = AtomicU32::new(0);

fn tagged(extra: usize) -> bool {
    extra == PROBE_TAG
}

unsafe extern "system" fn swallow_mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        if tagged(info.dwExtraInfo) {
            SWALLOWED_MOUSE.fetch_add(1, Ordering::SeqCst);
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn swallow_keys(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if tagged(info.dwExtraInfo) {
            SWALLOWED_KEYS.fetch_add(1, Ordering::SeqCst);
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// The other program's hooks, on a thread of their own that reads its
/// messages, as a hook thread must. Dropped, they are removed and the
/// thread ends.
#[derive(Debug)]
struct Swallower {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Swallower {
    fn install() -> Swallower {
        let (ready, installed) = channel();
        let thread = std::thread::spawn(move || {
            let module = unsafe { GetModuleHandleW(None) }.unwrap();
            let hooks = unsafe {
                [
                    SetWindowsHookExW(WH_MOUSE_LL, Some(swallow_mouse), Some(module.into()), 0),
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(swallow_keys), Some(module.into()), 0),
                ]
            };
            let hooks: Vec<_> = hooks.into_iter().map(Result::unwrap).collect();
            ready.send(unsafe { GetCurrentThreadId() }).unwrap();
            let mut msg = MSG::default();
            while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {}
            for hook in hooks {
                let _ = unsafe { UnhookWindowsHookEx(hook) };
            }
        });
        let thread_id = installed.recv().expect("the hooks are installed");
        Swallower {
            thread_id,
            thread: Some(thread),
        }
    }
}

impl Drop for Swallower {
    fn drop(&mut self) {
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default, World)]
pub struct WatchdogWorld {
    platform: Option<WinPlatform>,
    capture: Option<Box<dyn InputCapture>>,
    input: Option<Receiver<LocalInput>>,
    problems: Option<Receiver<PlatformProblem>>,
    watch: Option<WatchHandle>,
    swallower: Option<Swallower>,
    /// Input Mouser's capture saw while the desk user worked.
    seen: usize,
    /// What the watchdog wrote, for a failure to show.
    lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Drop for WatchdogWorld {
    fn drop(&mut self) {
        // The other program's hooks go first, then Mouser's.
        self.swallower.take();
        if let Some(mut capture) = self.capture.take() {
            capture.stop();
        }
    }
}

impl std::fmt::Debug for WatchdogWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchdogWorld")
            .field("capturing", &self.capture.is_some())
            .field("swallower", &self.swallower)
            .field("seen", &self.seen)
            .finish()
    }
}

/// One real, untagged event of each kind: a move of one pixel and back,
/// and a tap of F23.
fn desk_user_input() {
    let mouse = |dx: i32| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy: 0,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let key = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: F23,
                dwFlags: if up {
                    KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_SCANCODE
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let events = [mouse(1), mouse(-1), key(false), key(true)];
    let sent = unsafe { SendInput(&events, size_of::<INPUT>() as i32) };
    assert_eq!(sent as usize, events.len(), "the input was sent");
}

#[given(
    "software on a Windows computer intercepts Mouser's own checks of its hold on the mouse and keyboard"
)]
fn software_swallows_the_probes(world: &mut WatchdogWorld) {
    let lines = world.lines.clone();
    mouser_platform_windows::set_diagnostics(Some(std::sync::Arc::new(move |_, event, fields| {
        let fields: Vec<_> = fields.iter().map(|(k, v)| format!("{k}={v}")).collect();
        lines
            .lock()
            .unwrap()
            .push(format!("{event} {}", fields.join(" ")));
    })));
    let platform = WinPlatform::new();
    let (problems_tx, problems) = channel();
    world.watch = Some(
        platform
            .watch_problems(problems_tx)
            .expect("problems are watched"),
    );
    let mut capture = platform.capture().expect("a capture");
    let (tx, input) = channel();
    capture.start(tx).expect("the capture starts");
    // Installed after Mouser's hooks, so the system calls it first.
    world.swallower = Some(Swallower::install());
    world.capture = Some(capture);
    world.input = Some(input);
    world.problems = Some(problems);
    world.platform = Some(platform);
}

#[when("the desk user's mouse and keyboard keep working on that computer")]
fn the_desk_user_keeps_working(world: &mut WatchdogWorld) {
    let input = world.input.as_ref().expect("a capture");
    let end = Instant::now() + CYCLES;
    while Instant::now() < end {
        desk_user_input();
        std::thread::sleep(Duration::from_millis(200));
        world.seen += input.try_iter().count();
    }
}

#[then("Health shows no problem about that computer's hold on the mouse and keyboard")]
fn no_lost_hold(world: &mut WatchdogWorld) {
    let swallowed = (
        SWALLOWED_MOUSE.load(Ordering::SeqCst),
        SWALLOWED_KEYS.load(Ordering::SeqCst),
    );
    assert!(
        swallowed.0 >= 3 && swallowed.1 >= 3,
        "the watchdog probed through several cycles and every probe was swallowed: {swallowed:?}\n{}",
        world.lines.lock().unwrap().join("\n")
    );
    assert!(world.seen > 0, "Mouser's capture saw the desk user's input");
    let raised: Vec<_> = world
        .problems
        .as_ref()
        .expect("problems are watched")
        .try_iter()
        .filter(|p| *p == PlatformProblem::Raised(LOST_HOLD.into()))
        .collect();
    assert!(raised.is_empty(), "Health showed {raised:?}");
}
