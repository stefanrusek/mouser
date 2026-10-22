//! `Clipboard` on Wayland. The portal Clipboard on the RemoteDesktop session
//! is used first; when the grant lacks it, `data-control` is the fallback on
//! compositors that have it (KDE Plasma). GNOME before 51 offers nothing
//! else, and says so as a limitation.

use super::datacontrol::DataControl;
use super::desktop::{Desktop, Link};
use crate::bus::block_on;
use crate::limitation;
use mouser_platform::{Capability, ClipRep, Clipboard, Error, Limitation, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const GNOME_SHARING: &str = "Mouser cannot share the clipboard from this computer while it shares its mouse. This needs GNOME 51 or later.";

/// The first GNOME release whose clipboard works with input capture.
const GNOME_WITH_CLIPBOARD: u32 = 51;

/// Which service carries the clipboard.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Strategy {
    /// The portal Clipboard on the RemoteDesktop session.
    Portal,
    /// `ext-data-control` / `wlr-data-control`.
    DataControl,
    /// Nothing can reach the clipboard.
    Unavailable,
}

/// The portal when the grant includes it; else `data-control` unless this is
/// GNOME, which has none.
pub(crate) fn choose(portal_granted: bool, gnome: bool) -> Strategy {
    if portal_granted {
        Strategy::Portal
    } else if gnome {
        Strategy::Unavailable
    } else {
        Strategy::DataControl
    }
}

/// `XDG_CURRENT_DESKTOP` is a colon-separated list such as `ubuntu:GNOME`.
pub(crate) fn is_gnome(env: &HashMap<String, String>) -> bool {
    env.get("XDG_CURRENT_DESKTOP")
        .is_some_and(|d| d.split(':').any(|part| part.eq_ignore_ascii_case("GNOME")))
}

/// The major version in a shell version such as `46.2`.
pub(crate) fn major(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

/// The clipboard limitation of this desktop. An unknown GNOME version counts
/// as one before 51.
pub(crate) fn limitations(gnome: bool, shell_major: Option<u32>) -> Vec<Limitation> {
    if gnome && shell_major.is_none_or(|v| v < GNOME_WITH_CLIPBOARD) {
        vec![limitation(Capability::Clipboard, GNOME_SHARING)]
    } else {
        Vec::new()
    }
}

/// GNOME Shell's version, from the session bus.
pub(crate) async fn shell_major(conn: &zbus::Connection) -> Option<u32> {
    let shell = zbus::Proxy::new(
        conn,
        "org.gnome.Shell",
        "/org/gnome/Shell",
        "org.gnome.Shell",
    )
    .await
    .ok()?;
    let version: String = shell.get_property("ShellVersion").await.ok()?;
    major(&version)
}

enum Backend {
    Portal(Arc<Link>),
    DataControl(DataControl),
}

pub(crate) struct WaylandClipboard {
    desktop: Arc<Desktop>,
    gnome: bool,
    backend: Option<Backend>,
}

impl WaylandClipboard {
    pub(crate) fn new(desktop: Arc<Desktop>, env: &HashMap<String, String>) -> Self {
        WaylandClipboard {
            desktop,
            gnome: is_gnome(env),
            backend: None,
        }
    }

    /// Chosen on first use, once the grant shows whether it includes the
    /// clipboard (that can wait for the approval dialog).
    fn backend(&mut self) -> Result<&Backend> {
        if self.backend.is_none() {
            let link = self.desktop.link();
            self.backend = Some(match choose(link.clipboard().is_some(), self.gnome) {
                Strategy::Portal => Backend::Portal(link),
                Strategy::DataControl => Backend::DataControl(DataControl::start()?),
                Strategy::Unavailable => {
                    return Err(Error::Os(
                        "this desktop did not give Mouser access to the clipboard".into(),
                    ));
                }
            });
        }
        Ok(self.backend.as_ref().unwrap())
    }
}

impl Clipboard for WaylandClipboard {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        match self.backend()? {
            Backend::Portal(link) => {
                let link = link.clone();
                block_on(async move { link.clipboard().unwrap().read().await })
            }
            Backend::DataControl(control) => control.read(),
        }
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        match self.backend()? {
            Backend::Portal(link) => {
                let link = link.clone();
                block_on(async move { link.clipboard().unwrap().write(reps).await })
            }
            Backend::DataControl(control) => control.write(reps),
        }
    }

    fn change_count(&mut self) -> u64 {
        match self.backend() {
            Ok(Backend::Portal(link)) => link.clipboard().unwrap().change_count(),
            Ok(Backend::DataControl(control)) => control.change_count(),
            Err(_) => 0,
        }
    }
}
