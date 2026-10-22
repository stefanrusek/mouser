use crate::autostart::RunKey;
use mouser_platform::Autostart;
use std::path::PathBuf;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_BINARY, RRF_RT_REG_SZ, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::HSTRING;

/// Private Run / StartupApproved keys under `HKCU\Software\MouserTest\<test>-<pid>`,
/// deleted again on drop.
struct Keys {
    root: String,
    run: String,
    approved: String,
}

impl Keys {
    fn new(test: &str) -> Self {
        let root = format!(r"Software\MouserTest\{test}-{}", std::process::id());
        Keys {
            run: format!(r"{root}\Run"),
            approved: format!(r"{root}\StartupApproved\Run"),
            root,
        }
    }

    fn autostart(&self) -> RunKey {
        RunKey::new(
            &self.run,
            &self.approved,
            PathBuf::from(r"C:\Program Files\Mouser\Mouser.exe"),
        )
    }

    fn run_value(&self) -> Option<String> {
        let mut buf = [0u16; 260];
        let mut size = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(&self.run),
                &HSTRING::from("Mouser"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        (r == ERROR_SUCCESS).then(|| String::from_utf16_lossy(&buf[..size as usize / 2 - 1]))
    }

    /// What Windows writes when the user switches the entry on (0x02) or off (0x03)
    /// in Startup Apps.
    fn set_approval(&self, first_byte: u8) {
        let value = [first_byte, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(&self.approved),
                &HSTRING::from("Mouser"),
                REG_BINARY.0,
                Some(value.as_ptr().cast()),
                value.len() as u32,
            )
        };
        assert_eq!(r, ERROR_SUCCESS);
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&self.root)) };
        assert_eq!(r, ERROR_SUCCESS, "delete test keys");
    }
}

#[test]
fn enabling_writes_the_quoted_run_value_and_disabling_removes_it() {
    let keys = Keys::new("run");
    let mut autostart = keys.autostart();
    assert!(!autostart.enabled());
    autostart.set(true).unwrap();
    assert!(autostart.enabled());
    assert_eq!(
        keys.run_value().as_deref(),
        Some(r#""C:\Program Files\Mouser\Mouser.exe""#)
    );
    autostart.set(true).unwrap();
    autostart.set(false).unwrap();
    assert!(!autostart.enabled());
    assert_eq!(keys.run_value(), None);
    autostart.set(false).unwrap();
}

#[test]
fn startup_apps_choice_is_honoured() {
    let keys = Keys::new("approved");
    let mut autostart = keys.autostart();
    autostart.set(true).unwrap();
    keys.set_approval(0x03);
    assert!(!autostart.enabled(), "switched off in Startup Apps");
    keys.set_approval(0x02);
    assert!(autostart.enabled(), "switched back on in Startup Apps");
    keys.set_approval(0x03);
    autostart.set(true).unwrap();
    assert!(
        autostart.enabled(),
        "turning autostart on in Mouser clears the old choice"
    );
}

#[test]
fn approval_without_a_run_entry_is_not_enabled() {
    let keys = Keys::new("orphan");
    keys.set_approval(0x02);
    assert!(!keys.autostart().enabled());
}
