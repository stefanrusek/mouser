//! Starting the portal sessions: registered under `com.mouser.app`,
//! persisted until revoked (`persist_mode = 2`) with the single-use restore
//! token replaced after every `Start`.

use super::portal_clip::PortalClip;
use ashpd::AppID;
use ashpd::desktop::PersistMode;
use ashpd::desktop::Session;
use ashpd::desktop::input_capture::{
    Capabilities, CreateSession2Options, CreateSessionOptions, GetZonesOptions, InputCapture,
    StartOptions as CaptureStart,
};
use ashpd::desktop::remote_desktop::{
    DeviceType, RemoteDesktop, SelectDevicesOptions, StartOptions as RemoteStart,
};
use std::sync::Arc;
use zbus::Connection;

pub(crate) const APP_ID: &str = "com.mouser.app";

/// Hosts without the registry (portal before 1.19) work unregistered.
async fn register(conn: &Connection) {
    let _ = ashpd::register_host_app_with_connection(
        conn.clone(),
        AppID::try_from(APP_ID).expect("app id"),
    )
    .await;
}

/// A display of the capture zones: an offset and a size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Zone {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) struct CaptureSession {
    pub(crate) portal: InputCapture,
    pub(crate) session: Session<InputCapture>,
    /// The restore token to keep for the next login, if the portal gave one.
    pub(crate) token: Option<String>,
}

/// Version 2 portals create the session and then `Start` it (the dialog);
/// version 1 portals show the dialog from `CreateSession` and cannot persist.
pub(crate) async fn start_capture(conn: Connection, token: Option<String>) -> CaptureSession {
    register(&conn).await;
    let portal = InputCapture::with_connection(conn).await.unwrap();
    let capabilities = Capabilities::Keyboard | Capabilities::Pointer;
    let (session, token) = if portal.version() >= 2 {
        let session = portal
            .create_session2(CreateSession2Options::default())
            .await
            .unwrap();
        let started = portal
            .start(
                &session,
                None,
                CaptureStart::default()
                    .set_capabilities(capabilities)
                    .set_persist_mode(PersistMode::ExplicitlyRevoked)
                    .set_restore_token(token),
            )
            .await
            .unwrap()
            .response()
            .unwrap();
        (session, started.restore_token().map(str::to_owned))
    } else {
        let (session, _) = portal
            .create_session(
                None,
                CreateSessionOptions::default().set_capabilities(capabilities),
            )
            .await
            .unwrap();
        (session, None)
    };
    CaptureSession {
        portal,
        session,
        token,
    }
}

/// The zones and their set id.
pub(crate) async fn zones(capture: &CaptureSession) -> (Vec<Zone>, u32) {
    let zones = capture
        .portal
        .zones(&capture.session, GetZonesOptions::default())
        .await
        .unwrap()
        .response()
        .unwrap();
    let regions = zones
        .regions()
        .iter()
        .map(|r| Zone {
            x: r.x_offset(),
            y: r.y_offset(),
            width: r.width(),
            height: r.height(),
        })
        .collect();
    (regions, zones.zone_set())
}

pub(crate) struct DesktopSession {
    pub(crate) remote: RemoteDesktop,
    pub(crate) session: Arc<Session<RemoteDesktop>>,
    pub(crate) token: Option<String>,
    /// The clipboard, when the desktop offers one and the grant includes it.
    pub(crate) clipboard: Option<PortalClip>,
}

/// The RemoteDesktop grant: keyboard and pointer, with the clipboard asked up
/// front (a grant without it cannot be extended later), persisted until revoked.
pub(crate) async fn start_remote_desktop(
    conn: Connection,
    token: Option<String>,
) -> DesktopSession {
    register(&conn).await;
    let remote = RemoteDesktop::with_connection(conn.clone()).await.unwrap();
    let session = Arc::new(remote.create_session(Default::default()).await.unwrap());
    let clipboard = PortalClip::request(conn, session.clone()).await;
    remote
        .select_devices(
            &session,
            SelectDevicesOptions::default()
                .set_devices(DeviceType::Keyboard | DeviceType::Pointer)
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(token.as_deref()),
        )
        .await
        .unwrap()
        .response()
        .unwrap();
    let selected = remote
        .start(&session, None, RemoteStart::default())
        .await
        .unwrap()
        .response()
        .unwrap();
    DesktopSession {
        remote,
        session,
        token: selected.restore_token().map(str::to_owned),
        clipboard: clipboard.filter(|_| selected.is_clipboard_enabled()),
    }
}
