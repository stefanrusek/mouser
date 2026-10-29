use crate::uipi::*;
use windows::Win32::System::Threading::GetCurrentProcessId;

/// SECURITY_MANDATORY_MEDIUM_RID and SECURITY_MANDATORY_HIGH_RID.
const MEDIUM: u32 = 0x2000;
const HIGH: u32 = 0x3000;

fn own_pid() -> u32 {
    unsafe { GetCurrentProcessId() }
}

#[test]
fn this_process_has_a_standard_integrity_level() {
    let level = integrity_level(own_pid()).unwrap();
    assert!(level == MEDIUM || level == HIGH, "{level:#x}");
}

#[test]
fn a_process_outranks_only_those_below_it() {
    let own = integrity_level(own_pid()).unwrap();
    assert!(!outranks(own_pid(), own), "equal integrity");
    assert!(outranks(own_pid(), own - 0x1000), "a lower-integrity peer");
    assert!(
        !outranks(own_pid(), own + 0x1000),
        "a higher-integrity peer"
    );
}

#[test]
fn a_process_that_cannot_be_opened_counts_as_elevated() {
    // Process id 0 is the idle process: it cannot be opened.
    assert_eq!(integrity_level(0), None);
    assert!(outranks(0, HIGH));
}

#[test]
fn a_window_counts_as_outranking_when_mouser_cannot_read_its_own_level() {
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
    };
    use windows::core::w;
    // A window of this very process: the same level as Mouser's own.
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("uipi test"),
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
    let own = integrity_level(own_pid());
    let same_level = window_outranks(window, own);
    // Unknown, Mouser's level answers instead of failing: the window may outrank it.
    let unknown = window_outranks(window, None);
    let _ = unsafe { DestroyWindow(window) };
    assert!(!same_level);
    assert!(unknown);
}

#[test]
fn no_window_is_not_an_elevated_window() {
    assert!(!window_elevated(windows::Win32::Foundation::HWND::default()));
}

#[test]
fn the_foreground_window_is_classified_without_error() {
    // Whatever is in front of this test; a desktop without a foreground
    // window reports false.
    let _ = foreground_elevated();
}
