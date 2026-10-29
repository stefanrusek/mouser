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
mod diag;
mod displays;
mod dpi;
mod drag;
mod dropdir;
mod edge;
mod firewall;
mod foreground;
mod fullscreen;
mod inject;
mod keymap;
mod net;
mod netwatch;
mod permissions;
mod portable;
mod secrets;
mod uipi;
mod vfiles;
mod watchdog;

pub use capture::LOST_HOLD;
pub use diag::{Diagnostics, set_diagnostics};

use mouser_platform::{
    Autostart, Capability, Clipboard, FileDrag, InputCapture, InputInject, Limitation,
    LocalDisplay, NetInterface, Os, PermissionKind, PermissionState, Platform, PlatformInfo,
    PlatformProblem, Result, SecretStore, SecureInputHolder, SessionType, WatchHandle,
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
    /// Whether `watch_problems` also watches the firewall.
    firewall: bool,
    portable: Option<portable::Portable>,
}

/// Who hears the problems; `id` tells a listener from the one that replaced
/// it, so a stale handle does not unregister its successor.
struct Listener {
    id: u64,
    tx: Sender<PlatformProblem>,
}

impl WinPlatform {
    pub fn new() -> Self {
        Self::with_settings(Settings::system())
    }

    fn problem_sink(&self) -> capture::Problems {
        let sink = self.problems.clone();
        Arc::new(move |problem| {
            if let Some(listener) = sink.lock().unwrap().as_ref() {
                // A listener that has gone away just means nobody is listening.
                let _ = listener.tx.send(problem);
            }
        })
    }

    /// Like `problem_sink`, but only while listener `id` is still the one
    /// that hears the problems.
    fn problem_sink_for(&self, id: u64) -> capture::Problems {
        let sink = self.problems.clone();
        Arc::new(move |problem| {
            if let Some(listener) = sink.lock().unwrap().as_ref()
                && listener.id == id
            {
                let _ = listener.tx.send(problem);
            }
        })
    }

    #[cfg(test)]
    fn without_firewall_watch(mut self) -> Self {
        self.firewall = false;
        self
    }

    pub fn with_settings(settings: Settings) -> Self {
        // A crash while the pointer was hidden leaves it hidden; undo that.
        let _ = cursor::recover_at_startup();
        WinPlatform {
            settings,
            problems: Arc::new(Mutex::new(None)),
            listeners: AtomicU64::new(0),
            firewall: true,
            portable: None,
        }
    }
}

impl Default for WinPlatform {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn limitation(capability: Capability, text: &str) -> Limitation {
    Limitation {
        capability,
        text: text.into(),
    }
}

impl Platform for WinPlatform {
    fn info(&self) -> PlatformInfo {
        let mut limitations = vec![
            limitation(
                Capability::Inject,
                "Some games and remote-desktop apps ignore Mouser's input on this PC.",
            ),
            limitation(
                Capability::SecureAttention,
                "Ctrl-Alt-Del can't be sent to this PC yet.",
            ),
        ];
        limitations.extend(
            self.portable
                .iter()
                .flat_map(portable::Portable::limitations),
        );
        PlatformInfo {
            os: Os::Windows,
            version: windows_version(),
            session: SessionType::Native,
            limitations,
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
    /// mouse hook the system took away, Windows Firewall keeping other
    /// computers out); the app shows them. Replaces any earlier listener;
    /// dropping the handle stops this one.
    fn watch_problems(&self, tx: Sender<PlatformProblem>) -> Result<WatchHandle> {
        let id = self.listeners.fetch_add(1, Ordering::SeqCst) + 1;
        *self.problems.lock().unwrap() = Some(Listener { id, tx });
        let firewall = if self.firewall {
            let sink = self.problem_sink_for(id);
            let exe = std::env::current_exe().expect("current exe");
            Some(firewall::watch(exe, sink)?)
        } else {
            None
        };
        let slot = self.problems.clone();
        Ok(WatchHandle::new(move || {
            drop(firewall);
            let mut current = slot.lock().unwrap();
            if current.as_ref().is_some_and(|l| l.id == id) {
                *current = None;
            }
        }))
    }

    fn foreground_app(&self) -> Option<mouser_platform::ForegroundApp> {
        foreground::foreground_app()
    }

    fn secrets(&self) -> Box<dyn SecretStore> {
        match &self.portable {
            Some(p) => Box::new(portable::FileSecrets::new(&p.data_dir)),
            None => Box::new(secrets::DpapiStore::new(self.settings.secrets_dir.clone())),
        }
    }

    fn autostart(&self) -> Box<dyn Autostart> {
        if self.portable.is_some() {
            return Box::new(portable::NoAutostart);
        }
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
mod diag_tests;
#[cfg(test)]
mod displays_tests;
#[cfg(test)]
mod drag_tests;
#[cfg(test)]
mod firewall_tests;
#[cfg(test)]
mod foreground_tests;
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
mod portable_tests;
#[cfg(test)]
mod secrets_tests;

#[cfg(test)]
mod uipi_tests;
#[cfg(test)]
mod watchdog_tests;

/// Tests that send or capture real input share the machine's one input queue
/// and one set of hooks, so they take turns.
#[cfg(test)]
pub(crate) fn live_input() -> LiveInput {
    static LIVE: Mutex<()> = Mutex::new(());
    LiveInput {
        _guard: LIVE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    }
}

/// Holds the input lock; on release it waits a moment before letting the next
/// test have the input, so pointer movement the system is still delivering
/// (the VM's mouse integration among it) is not mistaken for that test's.
#[cfg(test)]
pub(crate) struct LiveInput {
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for LiveInput {
    fn drop(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}
