//! Portable mode (Story 13): nothing is installed, so secrets are plain files
//! under the drive's `MouserData/secrets` (a DPAPI blob would only open on the
//! computer that made it), there is no start-at-login entry, nothing is
//! written to the registry, and the app says what a portable run cannot do.
//!
//! On a volume with file permissions (NTFS, ReFS) the secrets folder is made
//! private to the current user and SYSTEM: inheritance is turned off and
//! nobody else is granted anything. Administrators are left out on purpose,
//! because the drive moves between computers. On FAT32/exFAT there are no
//! permissions to set, and the app says so.

use crate::inject::os;
use crate::secrets::{current_user_sid, protect_dir};
use crate::{WinPlatform, limitation};
use mouser_platform::{Autostart, Capability, Error, Limitation, Result, SecretStore};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use windows::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};
use windows::Win32::System::SystemServices::FILE_PERSISTENT_ACLS;
use windows::core::{HSTRING, PCWSTR};

/// The same sentence core adds for a portable run on Windows (D3 item 4).
pub(crate) const WINDOWS_LIMITATION: &str = "Portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.";

pub(crate) const UNPROTECTED: &str = "Secrets on this drive are not protected by file permissions; anyone with the drive can read them.";

/// Whether the secrets folder could be made private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Protection {
    Private,
    Unprotected,
}

/// What the platform knows about a portable run.
pub(crate) struct Portable {
    pub(crate) data_dir: PathBuf,
    pub(crate) protection: Protection,
}

impl Portable {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        let protection = prepare(&data_dir);
        Portable {
            data_dir,
            protection,
        }
    }

    pub(crate) fn limitations(&self) -> Vec<Limitation> {
        let mut all = vec![limitation(Capability::SecureAttention, WINDOWS_LIMITATION)];
        if self.protection == Protection::Unprotected {
            all.push(limitation(Capability::Capture, UNPROTECTED));
        }
        all
    }
}

impl WinPlatform {
    /// A portable run: `data_dir` is the drive's `MouserData/`.
    pub fn portable(data_dir: impl Into<PathBuf>) -> Self {
        Self::portable_with_settings(crate::Settings::system(), data_dir)
    }

    /// A portable run on the machine `settings` describe.
    pub fn portable_with_settings(settings: crate::Settings, data_dir: impl Into<PathBuf>) -> Self {
        let mut platform = Self::with_settings(settings);
        platform.portable = Some(Portable::new(data_dir.into()));
        platform
    }
}

/// The volume flags of the volume holding `path`.
pub(crate) fn volume_flags(path: &Path) -> Result<u32> {
    let mut root = [0u16; 261];
    let named = unsafe { GetVolumePathNameW(&HSTRING::from(path.as_os_str()), &mut root) };
    os("GetVolumePathNameW", named)?;
    let mut flags = 0u32;
    let root = PCWSTR(root.as_ptr());
    let asked = unsafe { GetVolumeInformationW(root, None, None, None, Some(&mut flags), None) };
    os("GetVolumeInformationW", asked)?;
    Ok(flags)
}

/// Makes `<data_dir>/secrets` and, where the volume has file permissions,
/// private to the current user and SYSTEM.
pub(crate) fn prepare(data_dir: &Path) -> Protection {
    prepare_with(data_dir, &volume_flags)
}

/// [`prepare`] with the volume check supplied. A volume that cannot be asked
/// is treated as having no permissions.
pub(crate) fn prepare_with(data_dir: &Path, flags: &dyn Fn(&Path) -> Result<u32>) -> Protection {
    let secrets = data_dir.join("secrets");
    std::fs::create_dir_all(&secrets).expect("create secrets dir");
    if flags(&secrets).is_ok_and(|f| f & FILE_PERSISTENT_ACLS != 0) {
        protect_dir(&secrets, &private_sddl());
        Protection::Private
    } else {
        Protection::Unprotected
    }
}

/// Full access for SYSTEM and the current user, nothing inherited.
pub(crate) fn private_sddl() -> String {
    format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{})", current_user_sid())
}

/// Characters a Windows file name cannot hold, and the escape character.
const ESCAPED: [char; 10] = ['%', '\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// A secret's name as a file name: forbidden characters become `%XX`.
pub(crate) fn file_name(secret: &str) -> String {
    secret
        .chars()
        .map(|c| {
            if ESCAPED.contains(&c) {
                format!("%{:02X}", c as u32)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// One file per secret in `<data dir>/secrets/`.
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

fn io(e: std::io::Error) -> Error {
    Error::Os(e.to_string())
}

impl SecretStore for FileSecrets {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        match std::fs::read(self.dir.join(file_name(name))) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        std::fs::write(self.dir.join(file_name(name)), value).map_err(io)
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        match std::fs::remove_file(self.dir.join(file_name(name))) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }
}

/// A portable run installs nothing, so it cannot start with the computer.
pub(crate) struct NoAutostart;

impl Autostart for NoAutostart {
    fn enabled(&self) -> bool {
        false
    }

    fn set(&mut self, _on: bool) -> Result<()> {
        Err(Error::Unsupported)
    }
}
