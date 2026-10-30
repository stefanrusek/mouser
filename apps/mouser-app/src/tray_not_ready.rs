//! The tray while UI mode waits for the session engine (T533, T534): the
//! service has not started it yet, or is starting it again after a user
//! switch. Only opening Mouser and quitting make sense then.

use mouser_platform::Os;
use mouser_proto::control::TrayConnection;

use crate::tray::{Appearance, Entry, Icon, OPEN_ID, QUIT_ID, TrayView};

/// The tray's first line while the session engine is away.
pub const SUMMARY: &str = "Mouser isn't ready yet";
/// Its hover text.
pub const TOOLTIP: &str = "Mouser is waiting for its service to start";

/// The tray on `os` in `appearance` while the session engine is away.
pub fn view(os: Os, appearance: Appearance) -> TrayView {
    TrayView {
        icon: Icon::choose(os, appearance, false, TrayConnection::Disconnected),
        summary: SUMMARY.into(),
        tooltip: TOOLTIP.into(),
        entries: vec![
            Entry {
                id: None,
                label: SUMMARY.into(),
            },
            Entry {
                id: Some(OPEN_ID),
                label: "Open Mouser".into(),
            },
            Entry {
                id: Some(QUIT_ID),
                label: "Quit Mouser".into(),
            },
        ],
    }
}

/// Calls `on_change` with whether the engine is there (`ready`), now and
/// each time that changes, until nobody can change it any more.
pub async fn follow(
    mut ready: tokio::sync::watch::Receiver<bool>,
    mut on_change: impl FnMut(bool),
) {
    on_change(*ready.borrow_and_update());
    while ready.changed().await.is_ok() {
        on_change(*ready.borrow_and_update());
    }
}
