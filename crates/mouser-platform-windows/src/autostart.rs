//! Autostart through `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Mouser`
//! (FR-011), honouring the user's choice in Task Manager / Settings > Startup,
//! which Windows keeps under `...\Explorer\StartupApproved\Run`.

use mouser_platform::{Autostart, Result};
use std::path::PathBuf;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};
use windows::core::HSTRING;

pub(crate) const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub(crate) const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const VALUE: &str = "Mouser";

pub(crate) struct RunKey {
    run_key: String,
    approved_key: String,
    exe: PathBuf,
}

impl RunKey {
    /// `run_key` and `approved_key` are paths under `HKEY_CURRENT_USER`.
    pub(crate) fn new(run_key: &str, approved_key: &str, exe: PathBuf) -> Self {
        RunKey {
            run_key: run_key.into(),
            approved_key: approved_key.into(),
            exe,
        }
    }
}

fn found(r: WIN32_ERROR) -> bool {
    assert!(
        r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND,
        "registry: {r:?}"
    );
    r == ERROR_SUCCESS
}

fn delete(key: &str) {
    found(unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            &HSTRING::from(VALUE),
        )
    });
}

impl Autostart for RunKey {
    /// The Run entry exists and Startup Apps has not switched it off (an odd first
    /// byte in its StartupApproved value means disabled).
    fn enabled(&self) -> bool {
        let run = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(&self.run_key),
                &HSTRING::from(VALUE),
                RRF_RT_REG_SZ,
                None,
                None,
                None,
            )
        };
        let mut approval = [0u8; 12];
        let mut size = approval.len() as u32;
        let approved = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(&self.approved_key),
                &HSTRING::from(VALUE),
                RRF_RT_REG_BINARY,
                None,
                Some(approval.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        found(run) && (!found(approved) || approval[0] & 1 == 0)
    }

    fn set(&mut self, on: bool) -> Result<()> {
        // A stale "disabled" choice would keep a freshly enabled entry from running.
        delete(&self.approved_key);
        if on {
            let command: Vec<u16> = format!("\"{}\"", self.exe.display())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let r = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(&self.run_key),
                    &HSTRING::from(VALUE),
                    REG_SZ.0,
                    Some(command.as_ptr().cast()),
                    (command.len() * 2) as u32,
                )
            };
            assert_eq!(r, ERROR_SUCCESS, "set Run value");
        } else {
            delete(&self.run_key);
        }
        Ok(())
    }
}
