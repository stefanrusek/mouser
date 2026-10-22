//! Wayland portal permissions. A portal is "granted" when the desktop offers
//! it: the approval dialog only appears once capture or injection starts, and
//! core starts neither while a permission is missing, so waiting for a restore
//! token would never end. It is "missing" only when the portal is absent. On
//! X11 nothing needs permission.

use crate::Settings;
use crate::bus::{block_on, connect};
use crate::session::{PortalFacts, probe_portals, session_type};
use crate::wayland::portal::{start_capture, start_remote_desktop};
use mouser_platform::{Error, PermissionKind, PermissionState, Result, SecretStore, SessionType};

pub(crate) const INPUT_CAPTURE_TOKEN: &str = "portal-token-input-capture";
pub(crate) const REMOTE_DESKTOP_TOKEN: &str = "portal-token-remote-desktop";

const ALL_KINDS: [PermissionKind; 6] = [
    PermissionKind::Accessibility,
    PermissionKind::InputMonitoring,
    PermissionKind::ScreenRecording,
    PermissionKind::FullDiskAccess,
    PermissionKind::InputCapturePortal,
    PermissionKind::RemoteDesktopPortal,
];

/// Where the restore token of a portal permission is kept.
fn token_name(kind: PermissionKind) -> Option<&'static str> {
    match kind {
        PermissionKind::InputCapturePortal => Some(INPUT_CAPTURE_TOKEN),
        PermissionKind::RemoteDesktopPortal => Some(REMOTE_DESKTOP_TOKEN),
        _ => None,
    }
}

pub(crate) fn report(
    session: SessionType,
    portals: &PortalFacts,
) -> Vec<(PermissionKind, PermissionState)> {
    ALL_KINDS
        .into_iter()
        .map(|kind| {
            let offered = match kind {
                PermissionKind::InputCapturePortal => Some(portals.input_capture.is_some()),
                PermissionKind::RemoteDesktopPortal => Some(portals.remote_desktop.is_some()),
                _ => None,
            };
            let state = match (session, offered) {
                (SessionType::Wayland, Some(true)) => PermissionState::Granted,
                (SessionType::Wayland, Some(false)) => PermissionState::Missing,
                _ => PermissionState::NotApplicable,
            };
            (kind, state)
        })
        .collect()
}

/// The portals the desktop offers; none without a session bus.
pub(crate) fn probe(env: &std::collections::HashMap<String, String>) -> PortalFacts {
    let env = env.clone();
    block_on(async move {
        match connect(&env).await {
            Ok(conn) => probe_portals(&conn).await,
            Err(_) => PortalFacts::default(),
        }
    })
}

/// Starts the portal session so the desktop shows its approval dialog, then
/// keeps the restore token it returns. The dialog can wait for the user, so
/// this returns at once and a thread finishes the job.
pub(crate) fn open_settings(
    settings: &Settings,
    mut store: Box<dyn SecretStore>,
    kind: PermissionKind,
) -> Result<()> {
    let name = token_name(kind).ok_or(Error::Unsupported)?;
    if session_type(&settings.env) != SessionType::Wayland {
        return Err(Error::Unsupported);
    }
    let env = settings.env.clone();
    let stored = store
        .get(name)
        .unwrap()
        .map(|t| String::from_utf8(t).expect("token is text"));
    std::thread::spawn(move || {
        let token = block_on(async move {
            let conn = connect(&env).await.expect("session bus");
            match kind {
                PermissionKind::InputCapturePortal => start_capture(conn, stored).await.token,
                _ => start_remote_desktop(conn, stored).await.token,
            }
        });
        if let Some(token) = token {
            store.set(name, token.as_bytes()).unwrap();
        }
    });
    Ok(())
}
