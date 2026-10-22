//! [`SecretStore`] of machine-scope DPAPI blobs, one file per secret under
//! `%ProgramData%\Mouser\secrets\`, so mouser-service can read the desk keys
//! before anyone logs in (W4).

use mouser_platform::{Error, Result, SecretStore};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_LOCAL_MACHINE, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    CryptUnprotectData,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};

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

/// The SID of the user running this process, as `S-1-5-21-...`.
pub(crate) fn current_user_sid() -> String {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).expect("OpenProcessToken");
        let mut buf = [0u64; 64];
        let mut len = 0u32;
        GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            512,
            &mut len,
        )
        .expect("GetTokenInformation(TokenUser)");
        CloseHandle(token).expect("CloseHandle");
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).expect("ConvertSidToStringSidW");
        let sid = text.to_string().expect("SID text");
        LocalFree(Some(HLOCAL(text.0.cast())));
        sid
    }
}

/// Creates `dir` with a protected DACL (nothing inherited) giving full access to
/// SYSTEM, Administrators and the installing user only (foundational-notes.md,
/// 2026-10-22): machine-scope DPAPI alone lets any local user who can read a
/// file decrypt it. Files created inside inherit these entries.
fn create_private_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(io)?;
    let sddl = format!(
        "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{})",
        current_user_sid()
    );
    protect_dir(dir, &sddl);
    Ok(())
}

fn io(e: std::io::Error) -> Error {
    Error::Os(e.to_string())
}

/// Where the secret `name` lives: one file, its name escaped the same way a
/// portable run escapes it (core's desk keys are `desk_key/<desk id>`).
fn path_of(dir: &Path, name: &str) -> PathBuf {
    dir.join(crate::portable::file_name(name))
}

/// Replaces the DACL of `dir` with the one `sddl` describes. Existing files
/// and folders below pick up the inheritable entries.
pub(crate) fn protect_dir(dir: &Path, sddl: &str) {
    unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &HSTRING::from(sddl),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .expect("secrets directory security descriptor");
        let (mut present, mut defaulted) = (BOOL(0), BOOL(0));
        let mut dacl: *mut ACL = std::ptr::null_mut();
        GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted).expect("DACL");
        let r = SetNamedSecurityInfoW(
            &HSTRING::from(dir.as_os_str()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(dacl),
            None,
        );
        assert!(r.is_ok(), "set secrets directory ACL: {r:?}");
        LocalFree(Some(HLOCAL(sd.0)));
    }
}

impl SecretStore for DpapiStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        match std::fs::read(path_of(&self.dir, name)) {
            Ok(blob) => Ok(Some(unprotect(&blob))),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        if !self.dir.exists() {
            create_private_dir(&self.dir)?;
        }
        std::fs::write(path_of(&self.dir, name), protect(value)).map_err(io)
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        match std::fs::remove_file(path_of(&self.dir, name)) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }
}
