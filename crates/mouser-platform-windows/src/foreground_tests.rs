use crate::foreground::*;
use crate::uipi::integrity_level;
use mouser_platform::{ForegroundApp, Platform};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
};
use windows::core::w;

/// SECURITY_MANDATORY_MEDIUM_RID, _HIGH_RID and _SYSTEM_RID.
const MEDIUM: u32 = 0x2000;
const HIGH: u32 = 0x3000;
const SYSTEM: u32 = 0x4000;

fn own_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

fn own_exe_name() -> String {
    let exe = std::env::current_exe().unwrap();
    exe.file_name().unwrap().to_string_lossy().into_owned()
}

/// What `app_of_window` says about a window of this very process, judged
/// against Mouser at level `own`.
fn app_of_own_window(own: Option<u32>) -> Option<ForegroundApp> {
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("foreground test"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )
    }
    .unwrap();
    let app = app_of_window(window, own);
    let _ = unsafe { DestroyWindow(window) };
    app
}

#[test]
fn no_window_is_no_app() {
    assert_eq!(app_of_window(HWND::default(), Some(MEDIUM)), None);
}

#[test]
fn an_app_is_named_by_its_executable_file_only() {
    let app = app_of_own_window(integrity_level(own_pid())).unwrap();
    assert_eq!(app.name, own_exe_name());
    assert!(!app.name.contains('\\'), "{}", app.name);
}

#[test]
fn an_app_outranks_mouser_only_from_above() {
    let own = integrity_level(own_pid()).unwrap();
    let outranks = |mouser| app_of_own_window(mouser).unwrap().outranks_mouser;
    assert!(!outranks(Some(own)), "the same level");
    assert!(outranks(Some(own - 0x1000)), "Mouser below the app");
    assert!(!outranks(Some(SYSTEM)), "Mouser running as SYSTEM");
    assert!(outranks(None), "Mouser's own level unknown");
}

#[test]
fn administrators_run_elevated_or_as_system() {
    assert!(!is_admin(Some(MEDIUM)));
    assert!(is_admin(Some(HIGH)));
    assert!(is_admin(Some(SYSTEM)));
    // A level that cannot be read is most likely a SYSTEM process's.
    assert!(is_admin(None));
    let own = integrity_level(own_pid());
    assert_eq!(
        app_of_own_window(own).unwrap().admin,
        is_admin(own),
        "this process"
    );
}

#[test]
fn an_app_whose_process_cannot_be_opened_is_named_from_a_process_snapshot() {
    // A real process, this one, with opening it made to fail.
    assert_eq!(name_of(own_pid(), |_| None), Some(own_exe_name()));
}

#[test]
fn a_process_that_has_exited_has_no_name() {
    // Process ids are multiples of four, so this one is in neither the
    // process table nor the snapshot.
    assert_eq!(name_of(u32::MAX, opened_name), None);
}

#[test]
fn the_platform_reads_the_app_in_front_without_error() {
    // Whatever is in front of this test, as core asks for it.
    let _ = crate::WinPlatform::new().foreground_app();
}
