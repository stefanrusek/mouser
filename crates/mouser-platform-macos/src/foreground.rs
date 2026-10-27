//! The application in front, for Mouser's diagnostic log (FR-003).

use mouser_platform::ForegroundApp;
use objc2_app_kit::NSWorkspace;

/// What the OS says; a seam so the mapping is testable.
pub(crate) trait Probe {
    /// `NSWorkspace.frontmostApplication`'s localized name; `None` with no
    /// frontmost application.
    fn frontmost_name(&self) -> Option<String>;
}

pub(crate) struct SystemProbe;

impl Probe for SystemProbe {
    fn frontmost_name(&self) -> Option<String> {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.localizedName().unwrap().to_string())
    }
}

/// macOS has no elevated applications that outrank Mouser's event tap.
pub(crate) fn foreground_app(probe: &dyn Probe) -> Option<ForegroundApp> {
    probe.frontmost_name().map(|name| ForegroundApp {
        name,
        admin: false,
        outranks_mouser: false,
    })
}
