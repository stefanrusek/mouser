//! Wayland portal permissions. A portal grant is "granted" while a restore
//! token is stored; on X11 nothing needs permission.

use crate::Settings;
use crate::bus::{block_on, connect};
use crate::session::session_type;
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
    store: &dyn SecretStore,
) -> Vec<(PermissionKind, PermissionState)> {
    ALL_KINDS
        .into_iter()
        .map(|kind| {
            let state = match (session, token_name(kind)) {
                (SessionType::Wayland, Some(name)) => {
                    if store.get(name).unwrap().is_some() {
                        PermissionState::Granted
                    } else {
                        PermissionState::Missing
                    }
                }
                _ => PermissionState::NotApplicable,
            };
            (kind, state)
        })
        .collect()
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
