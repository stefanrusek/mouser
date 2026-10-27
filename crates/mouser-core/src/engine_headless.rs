//! Headless engine (Story 12): Mouser running where no user is logged in,
//! at the macOS login window or as the Windows service, with no window and
//! no tray.
//!
//! The engine itself is unchanged; what differs is where it finds its
//! secrets. Nobody has unlocked a user keychain before login, so the host
//! gives the engine a machine-scope store ([`EngineParams::secrets`]) and a
//! machine-scope config directory holding the desk membership, both written
//! beforehand from the user's session by [`Engine::export_machine_scope`].
//! On Windows the platform's own store is already machine scope (DPAPI) and
//! only the directory is needed; on macOS the host uses [`FileSecrets`] in a
//! directory only the agent can read.

use crate::lock::LockExt;
use std::path::{Path, PathBuf};

use mouser_platform::SecretStore;

use crate::engine::Engine;
use crate::identity::DEVICE_KEY;
use crate::state::{self, desk_key_name};

/// A [`SecretStore`] of one private file per secret in a directory.
pub struct FileSecrets {
    dir: PathBuf,
}

impl FileSecrets {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name.replace('/', "%2F"))
    }
}

fn os(e: std::io::Error) -> mouser_platform::Error {
    mouser_platform::Error::Os(e.to_string())
}

impl SecretStore for FileSecrets {
    fn get(&self, name: &str) -> mouser_platform::Result<Option<Vec<u8>>> {
        match std::fs::read(self.path(name)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(os(e)),
        }
    }

    fn set(&mut self, name: &str, value: &[u8]) -> mouser_platform::Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(os)?;
        let mut file = std::fs::OpenOptions::new();
        file.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut file, 0o600);
        let mut file = file.open(self.path(name)).map_err(os)?;
        std::io::Write::write_all(&mut file, value).map_err(os)
    }

    fn delete(&mut self, name: &str) -> mouser_platform::Result<()> {
        match std::fs::remove_file(self.path(name)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(os(e)),
        }
    }
}

impl Engine {
    /// Copy what a headless engine needs to be this computer on this desk:
    /// the device key and the desk key into `machine`, and the desk
    /// membership into `dir/state.json`. Run it from the user's session as
    /// part of the privileged install step, and again after the desk or its
    /// key changes (joining, leaving, key rotation).
    pub fn export_machine_scope(&self, dir: &Path, machine: &mut dyn SecretStore) {
        let core = self.ctx.core.guard();
        let secrets = self.ctx.secrets.guard();
        let mut names = vec![DEVICE_KEY.to_string()];
        names.extend(core.state.desk.as_ref().map(|d| desk_key_name(&d.desk_id)));
        for name in names {
            let value = secrets
                .get(&name)
                .expect("secret store readable")
                .expect("secret stored");
            machine
                .set(&name, &value)
                .expect("machine-scope store writable");
        }
        core.state
            .save(&dir.join(state::FILE))
            .expect("machine-scope state writable");
    }
}
