//! Tray availability: stock GNOME has no StatusNotifier host.

use crate::limitation;
use mouser_platform::{Capability, Limitation};
use zbus::{Connection, fdo::DBusProxy};

pub(crate) const NO_TRAY: &str = "This desktop shows no tray icon for Mouser. Install the AppIndicator extension to get one; the tray menu actions are also in the Mouser window.";

pub(crate) async fn has_watcher(conn: &Connection) -> bool {
    let bus = DBusProxy::new(conn).await.expect("bus proxy");
    bus.name_has_owner(
        "org.kde.StatusNotifierWatcher"
            .try_into()
            .expect("bus name"),
    )
    .await
    .expect("name owner query")
}

pub(crate) fn limitations(has_watcher: bool) -> Vec<Limitation> {
    if has_watcher {
        Vec::new()
    } else {
        vec![limitation(Capability::Capture, NO_TRAY)]
    }
}
