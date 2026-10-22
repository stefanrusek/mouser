//! Windows backend for the `mouser-platform` seam.
//!
//! Phase US1 provides interface classification, the machine-scope DPAPI secret
//! store, Run-key autostart and (not applicable) permissions. Capture,
//! injection, clipboard, displays and the rest arrive with their own phases;
//! until then they report `Unsupported` and are declared as limitations
//! (rule 9), which is the contract-conformant answer for a capability a
//! backend does not yet have.
//!
//! Windows only: on any other system this crate is an empty library, so the
//! workspace builds everywhere.

#![cfg(windows)]

mod autostart;
mod net;
mod permissions;
mod secrets;

use mouser_platform::{
    Autostart, Capability, Clipboard, Error, FileDrag, InputCapture, InputInject, Limitation,
    LocalDisplay, NetInterface, Os, PermissionKind, PermissionState, Platform, PlatformInfo,
    Result, SecretStore, SecureInputHolder, SessionType, WatchHandle,
};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::UI::Shell::{FOLDERID_ProgramData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::w;

/// Where the platform keeps its state; tests point it at private locations.
pub struct Settings {
    /// Directory of the DPAPI secret files (`%ProgramData%\Mouser\secrets`).
    pub secrets_dir: PathBuf,
    /// Run key under `HKEY_CURRENT_USER`.
    pub run_key: String,
    /// Startup Apps approval key under `HKEY_CURRENT_USER`.
    pub approved_key: String,
    /// The executable the Run entry launches.
    pub exe: PathBuf,
}

impl Settings {
    /// The running machine.
    pub fn system() -> Self {
        Settings {
            secrets_dir: program_data().join(r"Mouser\secrets"),
            run_key: autostart::RUN_KEY.into(),
            approved_key: autostart::APPROVED_KEY.into(),
            exe: std::env::current_exe().expect("current exe"),
        }
    }
}

fn program_data() -> PathBuf {
    let p = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, None) }
        .expect("ProgramData folder");
    let path = PathBuf::from(unsafe { p.to_string() }.expect("ProgramData path"));
    unsafe { CoTaskMemFree(Some(p.0.cast())) };
    path
}

/// `major.minor.build` from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`.
fn windows_version() -> String {
    let key = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let dword = |name| {
        let mut v = 0u32;
        let mut size = 4u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key,
                name,
                RRF_RT_REG_DWORD,
                None,
                Some((&raw mut v).cast()),
                Some(&mut size),
            )
        };
        assert!(r.is_ok(), "read Windows version: {r:?}");
        v
    };
    let mut build = [0u16; 16];
    let mut size = 32u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key,
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(build.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    assert!(r.is_ok(), "read Windows build: {r:?}");
    let build = String::from_utf16_lossy(&build[..size as usize / 2 - 1]);
    format!(
        "{}.{}.{build}",
        dword(w!("CurrentMajorVersionNumber")),
        dword(w!("CurrentMinorVersionNumber"))
    )
}

/// The Windows implementation of [`Platform`].
pub struct WinPlatform {
    settings: Settings,
}

impl WinPlatform {
    pub fn new() -> Self {
        Self::with_settings(Settings::system())
    }

    pub fn with_settings(settings: Settings) -> Self {
        WinPlatform { settings }
    }
}

impl Default for WinPlatform {
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

impl Platform for WinPlatform {
    fn info(&self) -> PlatformInfo {
        PlatformInfo {
            os: Os::Windows,
            version: windows_version(),
            session: SessionType::Native,
            limitations: vec![
                limitation(
                    Capability::Capture,
                    "Sharing this PC's keyboard and mouse is not available yet.",
                ),
                limitation(
                    Capability::Inject,
                    "Controlling this PC from another computer is not available yet.",
                ),
                limitation(
                    Capability::Clipboard,
                    "Clipboard sharing is not available yet.",
                ),
                limitation(
                    Capability::FileDrag,
                    "Dragging files between computers is not available yet.",
                ),
            ],
        }
    }

    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        Err(Error::Unsupported)
    }

    fn watch_displays(&self, _tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        Err(Error::Unsupported)
    }

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        permissions::report()
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        permissions::open_settings(kind)
    }

    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        Err(Error::Unsupported)
    }

    fn inject(&self) -> Result<Box<dyn InputInject>> {
        Err(Error::Unsupported)
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        Err(Error::Unsupported)
    }

    fn secure_input(&self) -> Option<SecureInputHolder> {
        None
    }

    fn foreground_is_fullscreen(&self) -> bool {
        false
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        net::interfaces()
    }

    fn watch_network(&self, _tx: Sender<()>) -> Result<WatchHandle> {
        Err(Error::Unsupported)
    }

    fn foreground_elevated(&self) -> bool {
        false
    }

    fn secrets(&self) -> Box<dyn SecretStore> {
        Box::new(secrets::DpapiStore::new(self.settings.secrets_dir.clone()))
    }

    fn autostart(&self) -> Box<dyn Autostart> {
        Box::new(autostart::RunKey::new(
            &self.settings.run_key,
            &self.settings.approved_key,
            self.settings.exe.clone(),
        ))
    }

    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        None
    }
}

#[cfg(test)]
mod autostart_tests;
#[cfg(test)]
mod lib_tests;
#[cfg(test)]
mod net_tests;
#[cfg(test)]
mod permissions_tests;
#[cfg(test)]
mod secrets_tests;
