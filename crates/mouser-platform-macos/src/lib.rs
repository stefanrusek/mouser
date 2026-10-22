//! macOS backend for the `mouser-platform` seam.
//!
//! Provides permissions, interface classification, the Keychain secret store,
//! login-item autostart, displays, input capture and injection and Secure
//! Input detection. Clipboard and file drag are supported; capabilities a later phase adds report
//! `Unsupported` and are declared as limitations (rule 9) until it lands.

mod activity;
mod autostart;
mod capture;
mod clipboard;
mod displays;
mod drag;
mod dragwin;
mod echo;
mod fullscreen;
mod inject;
mod keymap;
pub mod login_agent;
mod net;
mod netwatch;
mod permissions;
mod portable;
mod secrets;
mod secure_input;
mod tap;

use mouser_platform::{
    Autostart, Capability, Clipboard, FileDrag, InputCapture, InputInject, Limitation,
    LocalDisplay, NetInterface, Os, PermissionKind, PermissionState, Platform, PlatformInfo,
    Result, SecretStore, SecureInputHolder, SessionType, WatchHandle,
};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::Sender;

/// Keychain service name under which Mouser stores its secrets.
pub const KEYCHAIN_SERVICE: &str = "com.mouser.app";

/// The macOS implementation of [`Platform`].
pub struct MacPlatform {
    open_program: PathBuf,
    echo: Arc<echo::Echo>,
    /// `MouserData/` of a portable run; `None` when installed.
    data_dir: Option<PathBuf>,
}

impl MacPlatform {
    /// Production platform: System Settings panes open through `/usr/bin/open`.
    pub fn new() -> Self {
        Self::with_open_program("/usr/bin/open")
    }

    /// Platform whose settings URLs are handed to `program` (called as
    /// `program <url>`) instead of `/usr/bin/open`; tests pass a harmless
    /// program so no System Settings window appears.
    pub fn with_open_program(program: impl Into<PathBuf>) -> Self {
        MacPlatform {
            open_program: program.into(),
            echo: Arc::default(),
            data_dir: None,
        }
    }
}

impl Default for MacPlatform {
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

fn product_version() -> String {
    let out = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .expect("sw_vers");
    String::from_utf8(out.stdout)
        .expect("sw_vers output")
        .trim()
        .to_string()
}

impl Platform for MacPlatform {
    fn info(&self) -> PlatformInfo {
        PlatformInfo {
            os: Os::MacOs,
            version: product_version(),
            session: SessionType::Native,
            limitations: [
                limitation(
                    Capability::Inject,
                    "Some games and remote-desktop apps ignore Mouser's input on this Mac.",
                ),
                limitation(
                    Capability::SecureAttention,
                    "Ctrl-Alt-Del can only be sent to Windows computers.",
                ),
            ]
            .into_iter()
            .chain(self.data_dir.iter().flat_map(|_| portable::limitations()))
            .collect(),
        }
    }

    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        displays::displays()
    }

    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        displays::watch(tx)
    }

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        permissions::report(&permissions::SystemProbe)
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        permissions::open_settings(&self.open_program, kind)
    }

    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        Ok(Box::new(capture::MacCapture::system(self.echo.clone())))
    }

    fn inject(&self) -> Result<Box<dyn InputInject>> {
        Ok(Box::new(inject::MacInject::system(self.echo.clone())))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        Ok(Box::new(clipboard::MacClipboard::new(
            clipboard::SystemBoard::new()?,
        )))
    }

    fn secure_input(&self) -> Option<SecureInputHolder> {
        secure_input::holder(&secure_input::SystemProbe)
    }

    fn foreground_is_fullscreen(&self) -> bool {
        fullscreen::foreground_is_fullscreen(&fullscreen::SystemProbe)
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        net::interfaces()
    }

    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle> {
        netwatch::watch(tx)
    }

    fn foreground_elevated(&self) -> bool {
        false
    }

    fn secrets(&self) -> Box<dyn SecretStore> {
        match &self.data_dir {
            Some(dir) => Box::new(portable::FileSecrets::new(dir)),
            None => Box::new(secrets::KeychainStore::new(KEYCHAIN_SERVICE)),
        }
    }

    fn autostart(&self) -> Box<dyn Autostart> {
        Box::new(autostart::LoginItem::new(autostart::SystemService(None)))
    }

    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        Some(Box::new(drag::MacFileDrag::new(
            dragwin::SystemProbe,
            dragwin::SystemStarter,
        )))
    }
}

#[cfg(test)]
mod activity_tests;
#[cfg(test)]
mod autostart_tests;
#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod displays_tests;
#[cfg(test)]
mod drag_tests;
#[cfg(test)]
mod echo_tests;
#[cfg(test)]
mod fullscreen_tests;
#[cfg(test)]
mod info_tests;
#[cfg(test)]
mod inject_tests;
#[cfg(test)]
mod keymap_tests;
#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod login_agent_tests;
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
mod secure_input_tests;
#[cfg(test)]
mod tap_tests;
