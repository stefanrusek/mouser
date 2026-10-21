//! Portable mode (Story 13): secrets live in files under `MouserData/` on the
//! drive instead of the Keychain, and the app states that macOS permissions
//! belong to the computer, not the drive.

use crate::{MacPlatform, limitation};
use mouser_platform::{Capability, Limitation, Result, SecretStore};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub(crate) const PERMISSIONS_LIMITATION: &str =
    "macOS permissions must be granted again on each computer you run Mouser from this drive on.";

pub(crate) fn limitations() -> Vec<Limitation> {
    vec![limitation(Capability::Capture, PERMISSIONS_LIMITATION)]
}

impl MacPlatform {
    /// Platform for a portable run: `data_dir` is the drive's `MouserData/`.
    pub fn portable(data_dir: impl Into<PathBuf>) -> Self {
        MacPlatform {
            data_dir: Some(data_dir.into()),
            ..Self::new()
        }
    }
}

/// One 0600 file per secret in `<data dir>/secrets/`.
pub(crate) struct FileSecrets {
    dir: PathBuf,
}

impl FileSecrets {
    pub(crate) fn new(data_dir: &Path) -> Self {
        FileSecrets {
            dir: data_dir.join("secrets"),
        }
    }
}

impl SecretStore for FileSecrets {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(std::fs::read(self.dir.join(name)).ok())
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.dir).expect("create secrets dir");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.dir.join(name))
            .expect("open secret file");
        file.write_all(value).expect("write secret");
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        // Deleting a missing secret is allowed.
        let _ = std::fs::remove_file(self.dir.join(name));
        Ok(())
    }
}
