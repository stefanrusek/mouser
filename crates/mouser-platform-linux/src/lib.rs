//! Linux backend for the `mouser-platform` seam.
//!
//! Session detection, portal permissions, interface classification, the
//! Secret Service / 0600-file secret store, XDG autostart and tray detection.
//! Capabilities the platform lacks report `Unsupported` (or `None`) and are
//! declared as limitations (rule 9).
//!
//! Linux only: on any other system this crate is an empty library, so the
//! workspace builds everywhere.

#![cfg(target_os = "linux")]

mod autostart;
mod bus;
mod clipboard;
mod drag;
mod keymap;
mod net;
mod netwatch;
mod permissions;
mod portable;
mod secrets;
mod session;
mod tray;
mod wayland;
mod x11;

use mouser_platform::{
    Autostart, Capability, Clipboard, FileDrag, InputCapture, InputInject, Limitation,
    LocalDisplay, NetInterface, Os, PermissionKind, PermissionState, Platform, PlatformInfo,
    Result, SecretStore, SecureInputHolder, SessionType, WatchHandle,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Everything the backend reads from the machine, so tests can aim it at a
/// private session bus and a scratch home directory.
#[derive(Clone)]
pub struct Settings {
    /// Environment variables (`XDG_SESSION_TYPE`, `WAYLAND_DISPLAY`,
    /// `XDG_CONFIG_HOME`, `HOME`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`).
    pub env: HashMap<String, String>,
    /// `/sys/class/net`.
    pub sys_class_net: PathBuf,
    /// `/etc/os-release`.
    pub os_release: PathBuf,
    /// The executable the autostart entry launches.
    pub exe: PathBuf,
    /// How long a Wayland input event waits for the desktop to offer a device.
    pub device_wait: Duration,
}

impl Settings {
    /// The running machine.
    pub fn system() -> Self {
        Settings {
            env: std::env::vars().collect(),
            sys_class_net: "/sys/class/net".into(),
            os_release: "/etc/os-release".into(),
            exe: std::env::current_exe().expect("current exe"),
            device_wait: Duration::from_secs(10),
        }
    }
}

/// The Linux implementation of [`Platform`].
pub struct LinuxPlatform {
    settings: Settings,
    /// `MouserData/` of a portable run; `None` when installed.
    data_dir: Option<PathBuf>,
    /// The RemoteDesktop grant input injection and the clipboard share.
    desktop: Arc<wayland::desktop::Desktop>,
    /// What the injector sent, so the capture does not report it back.
    echo: Arc<x11::echo::Echo>,
    /// The Wayland display zones once known.
    zones: wayland::capture::SharedZones,
}

impl LinuxPlatform {
    pub fn new() -> Self {
        Self::with_settings(Settings::system())
    }

    pub fn with_settings(settings: Settings) -> Self {
        Self::build(settings, None)
    }

    fn build(settings: Settings, data_dir: Option<PathBuf>) -> Self {
        let vault = secrets::Vault::new(&settings.env, data_dir.clone());
        LinuxPlatform {
            desktop: wayland::desktop::Desktop::new(settings.env.clone(), vault),
            settings,
            data_dir,
            echo: Arc::default(),
            zones: Arc::default(),
        }
    }

    fn session(&self) -> SessionType {
        session::session_type(&self.settings.env)
    }
}

impl Default for LinuxPlatform {
    fn default() -> Self {
        Self::new()
    }
}

fn limitation(capability: Capability, text: &str) -> Limitation {
    Limitation {
        capability,
        text: text.into(),
    }
}

impl Platform for LinuxPlatform {
    fn info(&self) -> PlatformInfo {
        let session = self.session();
        let wayland = session == SessionType::Wayland;
        let env = self.settings.env.clone();
        let gnome = wayland::clipboard::is_gnome(&env);
        let (portals, tray, shell) = bus::block_on(async move {
            match bus::connect(&env).await {
                Ok(conn) => (
                    if wayland {
                        session::probe_portals(&conn).await
                    } else {
                        session::PortalFacts::default()
                    },
                    tray::has_watcher(&conn).await,
                    if wayland && gnome {
                        wayland::clipboard::shell_major(&conn).await
                    } else {
                        None
                    },
                ),
                Err(_) => (session::PortalFacts::default(), false, None),
            }
        });
        let mut limitations = Vec::new();
        if wayland {
            limitations.extend(session::wayland_limitations(&portals));
            limitations.push(limitation(Capability::Capture, session::NO_FULLSCREEN));
            limitations.push(limitation(Capability::Capture, session::NO_QUICKJUMP));
            limitations.push(limitation(Capability::Capture, session::NO_FOREGROUND_APP));
            limitations.extend(wayland::clipboard::limitations(gnome, shell));
        }
        limitations.push(limitation(Capability::FileDrag, drag::NO_FILE_DRAG));
        limitations.push(limitation(
            Capability::SecureAttention,
            "Ctrl-Alt-Del can only be sent to Windows computers.",
        ));
        limitations.extend(tray::limitations(tray));
        limitations.extend(portable::limitations(self.data_dir.is_some(), session));
        PlatformInfo {
            os: Os::Linux,
            version: session::os_version(
                &std::fs::read_to_string(&self.settings.os_release).expect("read os-release"),
            ),
            session,
            limitations,
        }
    }

    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        match self.session() {
            SessionType::Wayland => {
                Ok(wayland::displays::displays(&self.settings.env, &self.zones))
            }
            _ => {
                let (conn, root) = x11::connect(&self.settings.env);
                Ok(x11::displays::displays(&conn, root))
            }
        }
    }

    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        match self.session() {
            // The zones are fixed once capture starts, so there is nothing to watch.
            SessionType::Wayland => Ok(WatchHandle::new(|| {})),
            _ => Ok(x11::displays::watch(&self.settings.env, tx)),
        }
    }

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        let session = self.session();
        let portals = if session == SessionType::Wayland {
            permissions::probe(&self.settings.env)
        } else {
            session::PortalFacts::default()
        };
        permissions::report(session, &portals)
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        permissions::open_settings(&self.settings, self.secrets(), kind)
    }

    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        match self.session() {
            SessionType::Wayland => Ok(Box::new(wayland::capture::WaylandCapture::new(
                &self.settings.env,
                self.zones.clone(),
            ))),
            _ => Ok(Box::new(x11::capture::X11Capture::new(
                &self.settings.env,
                self.echo.clone(),
            ))),
        }
    }

    fn inject(&self) -> Result<Box<dyn InputInject>> {
        match self.session() {
            SessionType::Wayland => Ok(Box::new(wayland::inject::WaylandInject::new(
                self.desktop.clone(),
                self.settings.device_wait,
            ))),
            _ => Ok(Box::new(x11::inject::X11Inject::new(
                &self.settings.env,
                self.echo.clone(),
            ))),
        }
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        match self.session() {
            SessionType::Wayland => Ok(Box::new(wayland::clipboard::WaylandClipboard::new(
                self.desktop.clone(),
                &self.settings.env,
            ))),
            _ => Ok(Box::new(x11::clipboard::X11Clipboard::new(
                &self.settings.env,
            ))),
        }
    }

    fn secure_input(&self) -> Option<SecureInputHolder> {
        None
    }

    fn foreground_is_fullscreen(&self) -> bool {
        match self.session() {
            SessionType::Wayland => false,
            _ => {
                let (conn, root) = x11::connect(&self.settings.env);
                x11::fullscreen::foreground_is_fullscreen(&conn, root)
            }
        }
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        net::interfaces(&self.settings.sys_class_net)
    }

    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle> {
        Ok(netwatch::watch(&self.settings.env, tx))
    }

    fn foreground_app(&self) -> Option<mouser_platform::ForegroundApp> {
        match self.session() {
            SessionType::Wayland => wayland::foreground_app(),
            _ => {
                let (conn, root) = x11::connect(&self.settings.env);
                x11::foreground::foreground_app(&conn, root, std::path::Path::new("/proc"))
            }
        }
    }

    fn secrets(&self) -> Box<dyn SecretStore> {
        secrets::Vault::new(&self.settings.env, self.data_dir.clone()).open()
    }

    fn autostart(&self) -> Box<dyn Autostart> {
        match self.data_dir {
            Some(_) => Box::new(portable::NoAutostart),
            None => Box::new(autostart::XdgAutostart::new(
                secrets::config_dir(&self.settings.env),
                &self.settings.exe,
            )),
        }
    }

    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        None
    }
}

#[cfg(test)]
mod autostart_tests;
#[cfg(test)]
mod bus_tests;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod drag_tests;
#[cfg(test)]
mod keymap_tests;
#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod net_tests;
#[cfg(test)]
mod netwatch_tests;
#[cfg(test)]
mod permissions_tests;
#[cfg(test)]
mod portable_tests;
#[cfg(test)]
mod secrets_tests;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tray_tests;
