//! Where Mouser keeps its files (contracts/config.md). With a `MouserData/`
//! directory beside the executable Mouser is portable (Story 13): config,
//! state, logs and secrets all live in it and nothing is written elsewhere.
//! Otherwise the per-OS config directory is used.

use std::path::{Path, PathBuf};

use mouser_platform::{Os, SecretStore};

use crate::engine_headless::FileSecrets;

/// The directory beside the executable that makes Mouser portable.
pub const PORTABLE_DIR: &str = "MouserData";

/// The environment variables the per-OS paths are built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Env {
    pub home: Option<PathBuf>,
    pub appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Self {
        let var = |name| std::env::var_os(name).map(PathBuf::from);
        Self {
            home: var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }),
            appdata: var("APPDATA"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
        }
    }
}

/// The directory holding `config.toml`, `state.json`, `logs/` and, when
/// portable, `secrets/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub dir: PathBuf,
    pub portable: bool,
}

impl Paths {
    /// Portable when `MouserData/` is a directory beside `exe`, else the
    /// per-OS config directory of `os`.
    pub fn resolve(exe: &Path, os: Os, env: &Env) -> Self {
        let beside = exe
            .parent()
            .expect("executable has a directory")
            .join(PORTABLE_DIR);
        if beside.is_dir() {
            return Self {
                dir: beside,
                portable: true,
            };
        }
        let need =
            |p: &Option<PathBuf>| p.clone().expect("the environment names the user's folders");
        let dir = match os {
            Os::MacOs => need(&env.home).join("Library/Application Support/Mouser"),
            Os::Windows => need(&env.appdata).join("Mouser"),
            Os::Linux => env
                .xdg_config_home
                .clone()
                .unwrap_or_else(|| need(&env.home).join(".config"))
                .join("mouser"),
        };
        Self {
            dir,
            portable: false,
        }
    }

    /// Portable keeps its secrets as files on the drive; an installed Mouser
    /// uses the platform's own store (`None`).
    pub fn secrets(&self) -> Option<Box<dyn SecretStore>> {
        self.portable
            .then(|| Box::new(FileSecrets::new(self.dir.join("secrets"))) as Box<dyn SecretStore>)
    }
}
