//! The cursor operations a grab needs, behind a seam so the grab sequence
//! and the cursor guard are testable without moving the real pointer.
//!
//! Every operation that can be refused by the OS returns the refusal as an
//! error; nothing here panics on an OS failure (002 V1).

use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::{Error, Result};

pub(crate) trait Cursor: Send + Sync {
    fn position(&self) -> (f64, f64);
    /// Fails when the point is on no display or the OS refuses.
    fn warp(&self, x: f64, y: f64) -> Result<()>;
    fn associate(&self, on: bool) -> Result<()>;
    fn hide(&self) -> Result<()>;
    fn show(&self) -> Result<()>;
    /// Whether the cursor is visible right now (`CGCursorIsVisible`).
    fn visible(&self) -> bool;
}

/// FR-002: the OS refusing a warp is an error the caller sees, not a panic.
pub(crate) fn warp_refused(code: core_graphics::base::CGError) -> Error {
    Error::Os(format!("CGWarpMouseCursorPosition failed: {code}"))
}

/// The OS refusing a named cursor call, as an error naming the call and code.
pub(crate) fn os_refused(call: &str, code: core_graphics::base::CGError) -> Error {
    Error::Os(format!("{call} failed: {code}"))
}

pub(crate) struct SystemCursor;

impl Cursor for SystemCursor {
    fn position(&self) -> (f64, f64) {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap();
        let at = CGEvent::new(source).unwrap().location();
        (at.x, at.y)
    }
    fn warp(&self, x: f64, y: f64) -> Result<()> {
        let at = CGPoint::new(x, y);
        let on_a_display = CGDisplay::display_count_with_point(at).is_ok_and(|count| count > 0);
        if !on_a_display {
            return Err(Error::Os(format!("({x}, {y}) is on no display")));
        }
        CGDisplay::warp_mouse_cursor_position(at).map_err(warp_refused)
    }
    fn associate(&self, on: bool) -> Result<()> {
        CGDisplay::associate_mouse_and_mouse_cursor_position(on)
            .map_err(|code| os_refused("CGAssociateMouseAndMouseCursorPosition", code))
    }
    fn hide(&self) -> Result<()> {
        allow_hiding_in_background()?;
        CGDisplay::main()
            .hide_cursor()
            .map_err(|code| os_refused("CGDisplayHideCursor", code))
    }
    fn show(&self) -> Result<()> {
        CGDisplay::main()
            .show_cursor()
            .map_err(|code| os_refused("CGDisplayShowCursor", code))
    }
    fn visible(&self) -> bool {
        // SAFETY: a plain C call with no arguments.
        unsafe { CGCursorIsVisible() != 0 }
    }
}

// Private CoreGraphics (SkyLight) calls, as Synergy/Barrier use: without
// `SetsCursorInBackground`, CGDisplayHideCursor does nothing unless the
// caller is the frontmost app, and Mouser (a menu-bar app) never is.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(
        connection: i32,
        target: i32,
        key: core_foundation::string::CFStringRef,
        value: core_foundation::base::CFTypeRef,
    ) -> i32;
    /// Public CoreGraphics: `boolean_t CGCursorIsVisible(void)`.
    fn CGCursorIsVisible() -> u32;
}

/// Let this process hide the cursor while another app is frontmost.
pub(crate) fn allow_hiding_in_background() -> Result<()> {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    let key = CFString::from_static_string("SetsCursorInBackground");
    let value = CFBoolean::true_value();
    // SAFETY: plain C calls with valid, retained CF objects.
    let error = unsafe {
        let connection = _CGSDefaultConnection();
        CGSSetConnectionProperty(
            connection,
            connection,
            key.as_concrete_TypeRef(),
            value.as_CFTypeRef(),
        )
    };
    background_hiding_result(error)
}

/// The result of `CGSSetConnectionProperty(SetsCursorInBackground)`.
pub(crate) fn background_hiding_result(error: i32) -> Result<()> {
    if error == 0 {
        Ok(())
    } else {
        Err(os_refused(
            "CGSSetConnectionProperty(SetsCursorInBackground)",
            error,
        ))
    }
}
