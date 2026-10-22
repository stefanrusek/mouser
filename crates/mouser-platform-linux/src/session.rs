//! Session type and Wayland portal capability detection.

use crate::limitation;
use ashpd::desktop::{input_capture::InputCapture, remote_desktop::RemoteDesktop};
use mouser_platform::{Capability, Limitation, SessionType};
use std::collections::HashMap;
use zbus::Connection;

const KEYBOARD: u32 = 1;
const POINTER: u32 = 2;

pub(crate) const NO_CAPTURE: &str = "This desktop cannot share its mouse and keyboard with other computers. Mouser needs GNOME 46 or later or KDE Plasma 6.1 or later on Wayland.";
pub(crate) const NO_KEYBOARD_CAPTURE: &str =
    "This desktop cannot share its keyboard with other computers.";
pub(crate) const NO_POINTER_CAPTURE: &str =
    "This desktop cannot share its mouse with other computers.";
pub(crate) const APPROVE_EACH_LOGIN: &str = "This desktop asks you to approve Mouser each time you sign in before it can share this computer's mouse and keyboard.";
pub(crate) const NO_FULLSCREEN: &str = "Mouser cannot tell when an app is full-screen on Wayland, so the pointer may still cross to another computer during a game or video.";
pub(crate) const NO_QUICKJUMP: &str = "QuickJump and hotkey crossings can't start from this computer on Wayland; push the pointer off the screen edge instead.";
pub(crate) const NO_INJECT: &str = "This desktop does not let Mouser move the mouse or type here, so other computers cannot control this one.";

/// `XDG_SESSION_TYPE` decides; without it a `WAYLAND_DISPLAY` means Wayland.
pub(crate) fn session_type(env: &HashMap<String, String>) -> SessionType {
    match env.get("XDG_SESSION_TYPE").map(String::as_str) {
        Some("wayland") => SessionType::Wayland,
        Some("x11") => SessionType::X11,
        _ if env.contains_key("WAYLAND_DISPLAY") => SessionType::Wayland,
        _ => SessionType::X11,
    }
}

/// What the Wayland portals offer; `None` means the portal is absent.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PortalFacts {
    /// Raw `SupportedCapabilities` of the InputCapture portal (read as a plain
    /// integer: GNOME sets bits ashpd cannot decode).
    pub(crate) input_capture: Option<u32>,
    pub(crate) remote_desktop: Option<u32>,
}

async fn remote_desktop(conn: &Connection) -> Option<u32> {
    let portal = RemoteDesktop::with_connection(conn.clone()).await.ok()?;
    let types = portal.available_device_types().await.ok()?;
    Some(types.bits())
}

async fn input_capture(conn: &Connection) -> Option<u32> {
    let portal = InputCapture::with_connection(conn.clone()).await.ok()?;
    portal
        .get_property::<u32>("SupportedCapabilities")
        .await
        .ok()
}

pub(crate) async fn probe_portals(conn: &Connection) -> PortalFacts {
    PortalFacts {
        input_capture: input_capture(conn).await,
        remote_desktop: remote_desktop(conn).await,
    }
}

pub(crate) fn wayland_limitations(facts: &PortalFacts) -> Vec<Limitation> {
    let mut out = Vec::new();
    match facts.input_capture {
        None => out.push(limitation(Capability::Capture, NO_CAPTURE)),
        Some(caps) => {
            if caps & KEYBOARD == 0 {
                out.push(limitation(Capability::Capture, NO_KEYBOARD_CAPTURE));
            }
            if caps & POINTER == 0 {
                out.push(limitation(Capability::Capture, NO_POINTER_CAPTURE));
            }
            out.push(limitation(Capability::Capture, APPROVE_EACH_LOGIN));
        }
    }
    if facts.remote_desktop.is_none() {
        out.push(limitation(Capability::Inject, NO_INJECT));
    }
    out
}

/// `NAME VERSION_ID` from `os-release` text, e.g. `Ubuntu 24.04`.
pub(crate) fn os_version(os_release: &str) -> String {
    let field = |key: &str| {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_string())
    };
    format!(
        "{} {}",
        field("NAME").expect("os-release NAME"),
        field("VERSION_ID").expect("os-release VERSION_ID")
    )
}
