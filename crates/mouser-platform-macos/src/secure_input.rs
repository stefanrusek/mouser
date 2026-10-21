//! Secure Input detection: `IsSecureEventInputEnabled()` plus the holder's
//! pid from the undocumented `kCGSSessionSecureInputPID` key of
//! `CGSessionCopyCurrentDictionary` (FINDINGS-macos-input section 4).

use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use mouser_platform::SecureInputHolder;
use objc2_app_kit::NSRunningApplication;
use std::path::Path;

/// Shown when the OS does not say who holds Secure Input.
const GENERIC_HOLDER: &str = "another application";

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

/// What the OS can say about Secure Input; a seam so the mapping is testable.
pub(crate) trait Probe {
    fn enabled(&self) -> bool;
    /// `kCGSSessionSecureInputPID`; `None` when the key is absent.
    fn holder_pid(&self) -> Option<u32>;
    /// `NSRunningApplication.localizedName`.
    fn app_name(&self, pid: u32) -> Option<String>;
    /// `proc_pidpath`.
    fn process_path(&self, pid: u32) -> Option<String>;
}

pub(crate) struct SystemProbe;

impl Probe for SystemProbe {
    fn enabled(&self) -> bool {
        unsafe { IsSecureEventInputEnabled() }
    }

    fn holder_pid(&self) -> Option<u32> {
        let raw = unsafe { CGSessionCopyCurrentDictionary() };
        assert!(!raw.is_null(), "no current login session");
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_create_rule(raw) };
        let pid = dict.find(CFString::new("kCGSSessionSecureInputPID"))?;
        pid.downcast::<CFNumber>()?.to_i64().map(|pid| pid as u32)
    }

    fn app_name(&self, pid: u32) -> Option<String> {
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)?;
        Some(app.localizedName()?.to_string())
    }

    fn process_path(&self, pid: u32) -> Option<String> {
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let len =
            unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
        (len > 0).then(|| String::from_utf8_lossy(&buf[..len as usize]).into_owned())
    }
}

/// The application holding Secure Input, or `None` when it is off.
pub(crate) fn holder(probe: &dyn Probe) -> Option<SecureInputHolder> {
    if !probe.enabled() {
        return None;
    }
    let Some(pid) = probe.holder_pid() else {
        return Some(SecureInputHolder {
            pid: 0,
            app_name: GENERIC_HOLDER.into(),
        });
    };
    let app_name = probe
        .app_name(pid)
        .or_else(|| {
            let path = probe.process_path(pid)?;
            let file = Path::new(&path).file_name()?;
            Some(file.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| GENERIC_HOLDER.into());
    Some(SecureInputHolder { pid, app_name })
}
