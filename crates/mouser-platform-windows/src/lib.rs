//! Windows backend for the `mouser-platform` seam.
//!
//! Interface classification and network watch, the machine-scope DPAPI secret
//! store, Run-key autostart, displays, input capture and injection, elevation
//! detection, full-screen detection and the clipboard. File drag and portable
//! mode arrive with their own phases; a capability a backend does not yet
//! have reports `Unsupported` and is declared as a limitation (rule 9).
//!
//! Windows only: on any other system this crate is an empty library, so the
//! workspace builds everywhere.

#![cfg(windows)]

mod autostart;
mod capture;
mod clipboard;
mod cursor;
mod displays;
mod dpi;
mod drag;
mod dropdir;
mod edge;
mod fullscreen;
mod inject;
mod keymap;
mod net;
mod netwatch;
mod permissions;
mod secrets;
mod uipi;
mod vfiles;

use mouser_platform::{
    Autostart, Capability, Clipboard, FileDrag, InputCapture, InputInject, Limitation,
    LocalDisplay, NetInterface, Os, PermissionKind, PermissionState, Platform, PlatformInfo,
    Result, SecretStore, SecureInputHolder, SessionType, WatchHandle,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
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
    problems: Arc<Mutex<Option<Listener>>>,
    listeners: AtomicU64,
}

/// Who hears the problems; `id` tells a listener from the one that replaced
/// it, so a stale handle does not unregister its successor.
struct Listener {
    id: u64,
    tx: Sender<String>,
}

impl WinPlatform {
    pub fn new() -> Self {
        Self::with_settings(Settings::system())
    }

    fn problem_sink(&self) -> capture::Problems {
        let sink = self.problems.clone();
        Arc::new(move |text| {
            if let Some(listener) = sink.lock().unwrap().as_ref() {
                // A listener that has gone away just means nobody is listening.
                let _ = listener.tx.send(text);
            }
        })
    }

    pub fn with_settings(settings: Settings) -> Self {
        // A crash while the pointer was hidden leaves it hidden; undo that.
        let _ = cursor::recover_at_startup();
        WinPlatform {
            settings,
            problems: Arc::new(Mutex::new(None)),
            listeners: AtomicU64::new(0),
        }
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
                    Capability::Inject,
                    "Some games and remote-desktop apps ignore Mouser's input on this PC.",
                ),
                limitation(
                    Capability::SecureAttention,
                    "Ctrl-Alt-Del can't be sent to this PC yet.",
                ),
            ],
        }
    }

    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        displays::displays()
    }

    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        displays::watch(tx)
    }

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        permissions::report()
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        permissions::open_settings(kind)
    }

    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        Ok(Box::new(capture::WinCapture::new(self.problem_sink())))
    }

    fn inject(&self) -> Result<Box<dyn InputInject>> {
        Ok(Box::new(inject::WinInject::new(None)))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        Ok(Box::new(clipboard::WinClipboard::new()))
    }

    fn secure_input(&self) -> Option<SecureInputHolder> {
        None
    }

    fn foreground_is_fullscreen(&self) -> bool {
        fullscreen::foreground_is_fullscreen()
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        net::interfaces()
    }

    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle> {
        netwatch::watch(tx)
    }

    /// Where Mouser reports problems it noticed on this PC (a keyboard or
    /// mouse hook the system took away); the app shows them. Replaces any
    /// earlier listener; dropping the handle stops this one.
    fn watch_problems(&self, tx: Sender<String>) -> Result<WatchHandle> {
        let id = self.listeners.fetch_add(1, Ordering::SeqCst) + 1;
        *self.problems.lock().unwrap() = Some(Listener { id, tx });
        let slot = self.problems.clone();
        Ok(WatchHandle::new(move || {
            let mut current = slot.lock().unwrap();
            if current.as_ref().is_some_and(|l| l.id == id) {
                *current = None;
            }
        }))
    }

    fn foreground_elevated(&self) -> bool {
        uipi::foreground_elevated()
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
        Some(Box::new(drag::WinFileDrag::new()))
    }
}

#[cfg(test)]
mod autostart_tests;
#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod cursor_tests;
#[cfg(test)]
mod displays_tests;
#[cfg(test)]
mod drag_tests;
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
mod net_tests;
#[cfg(test)]
mod netwatch_tests;
#[cfg(test)]
mod permissions_tests;
#[cfg(test)]
mod secrets_tests;

#[cfg(test)]
mod uipi_tests;

/// Tests that send or capture real input share the machine's one input queue
/// and one set of hooks, so they take turns.
#[cfg(test)]
pub(crate) fn live_input() -> std::sync::MutexGuard<'static, ()> {
    static LIVE: Mutex<()> = Mutex::new(());
    LIVE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
