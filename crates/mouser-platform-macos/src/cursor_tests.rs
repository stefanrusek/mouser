use crate::cursor::{Cursor, SystemCursor, background_hiding_result, os_refused, refused_by};
use mouser_platform::Error;

#[test]
fn the_window_server_lets_mouser_hide_the_cursor_from_the_background() {
    // Without this, a grab leaves the cursor showing on the computer the
    // user left (Mouser is never the frontmost app). It changes no state
    // other than this process's permission to hide the cursor.
    crate::cursor::allow_hiding_in_background().unwrap();
    crate::cursor::allow_hiding_in_background().unwrap();
}

#[test]
fn warping_to_a_point_on_no_display_is_an_error_not_a_panic() {
    // Refused before any CoreGraphics call that could move the pointer.
    let refused = SystemCursor.warp(10_000_000.0, 10_000_000.0);
    assert!(matches!(refused, Err(Error::Os(_))));
}

#[test]
fn an_os_refused_warp_becomes_an_os_error_naming_the_code() {
    assert_eq!(
        crate::cursor::warp_refused(1001),
        Error::Os("CGWarpMouseCursorPosition failed: 1001".into())
    );
}

#[test]
fn a_refused_background_hiding_property_is_an_error_naming_it() {
    assert_eq!(background_hiding_result(0), Ok(()));
    assert_eq!(
        background_hiding_result(1001),
        Err(Error::Os(
            "CGSSetConnectionProperty(SetsCursorInBackground) failed: 1001".into()
        ))
    );
}

#[test]
fn a_refused_cursor_call_is_an_os_error_naming_the_call_and_code() {
    assert_eq!(
        os_refused("CGDisplayHideCursor", 1000),
        Error::Os("CGDisplayHideCursor failed: 1000".into())
    );
}

#[test]
fn the_system_cursor_reports_whether_it_is_visible_without_changing_it() {
    let before = SystemCursor.visible();
    assert_eq!(SystemCursor.visible(), before);
}

#[test]
fn a_refused_call_handed_to_map_err_names_the_call_and_code() {
    assert_eq!(
        refused_by("CGDisplayShowCursor")(1002),
        Error::Os("CGDisplayShowCursor failed: 1002".into())
    );
}
