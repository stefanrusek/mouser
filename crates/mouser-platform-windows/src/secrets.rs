//! [`SecretStore`] of machine-scope DPAPI blobs, one file per secret under
//! `%ProgramData%\Mouser\secrets\`, so mouser-service can read the desk keys
//! before anyone logs in (W4).

use mouser_platform::{Result, SecretStore};
use std::io::ErrorKind;
use std::path::PathBuf;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_LOCAL_MACHINE, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    CryptUnprotectData,
};
use windows::core::PCWSTR;

pub(crate) struct DpapiStore {
    dir: PathBuf,
}

impl DpapiStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        DpapiStore { dir }
    }
}

/// Copies a DPAPI output blob and frees it.
fn take(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let bytes = unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec();
    unsafe { LocalFree(Some(HLOCAL(blob.pbData.cast()))) };
    bytes
}

fn input(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr().cast_mut(),
    }
}

fn protect(data: &[u8]) -> Vec<u8> {
    let mut out = CRYPT_INTEGER_BLOB::default();
    let flags = CRYPTPROTECT_LOCAL_MACHINE | CRYPTPROTECT_UI_FORBIDDEN;
    unsafe {
        CryptProtectData(
            &input(data),
            PCWSTR::null(),
            None,
            None,
            None,
            flags,
            &mut out,
        )
    }
    .expect("CryptProtectData");
    take(out)
}

fn unprotect(blob: &[u8]) -> Vec<u8> {
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input(blob),
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .expect("CryptUnprotectData");
    take(out)
}

impl SecretStore for DpapiStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let blob = match std::fs::read(self.dir.join(name)) {
            Ok(blob) => blob,
            Err(e) => {
                assert_eq!(e.kind(), ErrorKind::NotFound, "read secret {name}: {e}");
                return Ok(None);
            }
        };
        Ok(Some(unprotect(&blob)))
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        std::fs::create_dir_all(&self.dir).expect("create secrets dir");
        std::fs::write(self.dir.join(name), protect(value)).expect("write secret");
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        if let Err(e) = std::fs::remove_file(self.dir.join(name)) {
            assert_eq!(e.kind(), ErrorKind::NotFound, "delete secret {name}: {e}");
        }
        Ok(())
    }
}
