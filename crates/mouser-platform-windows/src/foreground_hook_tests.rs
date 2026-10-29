use crate::foreground_hook::*;
use crate::inject::{MOUSER_TAG, key_input, send};
use mouser_platform::ForegroundApp;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, EVENT_SYSTEM_FOREGROUND, MSG, PM_REMOVE,
    PeekMessageW, SetForegroundWindow, WINDOW_EX_STYLE, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;

fn own_exe_name() -> String {
    let exe = std::env::current_exe().unwrap();
    exe.file_name().unwrap().to_string_lossy().into_owned()
}

/// What the hook reported, in order.
type Seen = Rc<RefCell<Vec<Option<ForegroundApp>>>>;

fn hook() -> (ForegroundHook, Seen) {
    let seen: Seen = Rc::default();
    let into = seen.clone();
    let hook = ForegroundHook::install(Box::new(move |app| into.borrow_mut().push(app)))
        .expect("the foreground hook is installed");
    (hook, seen)
}

fn own_window(style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE) -> HWND {
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("foreground hook test"),
            WS_POPUP | style,
            0,
            0,
            40,
            40,
            None,
            None,
            None,
            None,
        )
    }
    .unwrap()
}

#[test]
fn each_foreground_change_reports_the_app_now_in_front() {
    let (_hook, seen) = hook();
    let window = own_window(Default::default());
    // As the system calls it, once per change.
    let report = |window: HWND| unsafe {
        on_event(
            HWINEVENTHOOK::default(),
            EVENT_SYSTEM_FOREGROUND,
            window,
            0,
            0,
            0,
            0,
        )
    };
    report(window);
    report(HWND::default());
    let _ = unsafe { DestroyWindow(window) };
    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].as_ref().map(|a| a.name.clone()),
        Some(own_exe_name())
    );
    assert_eq!(seen[1], None, "nothing in front");
}

#[test]
fn a_real_foreground_change_reaches_the_hook() {
    let _live = crate::live_input();
    let (hook, seen) = hook();
    let window = own_window(WS_VISIBLE);
    // Windows lets a program bring its window to the front just after it
    // sent input; a tap of Alt is the least a press can do.
    send(&[
        key_input(0x38, false, true, MOUSER_TAG),
        key_input(0x38, false, false, MOUSER_TAG),
    ])
    .unwrap();
    let _ = unsafe { SetForegroundWindow(window) };
    let ours = || {
        seen.borrow()
            .iter()
            .any(|app| app.as_ref().is_some_and(|a| a.name == own_exe_name()))
    };
    // Out-of-context events arrive while this thread reads its messages.
    let end = Instant::now() + Duration::from_secs(5);
    let mut msg = MSG::default();
    while !ours() && Instant::now() < end {
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe { DispatchMessageW(&msg) };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let got = ours();
    drop(hook);
    let _ = unsafe { DestroyWindow(window) };
    assert!(got, "{:?}", seen.borrow());
}
