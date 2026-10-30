//! What doesn't work on this PC, by how Mouser runs here (data-model.md
//! "Limitation texts"; Story 5 sc. 3, 8; FR-004). Every text describes this
//! computer.

use crate::limitation;
use mouser_platform::{Capability, Limitation};
use windows::Win32::System::Registry::{HKEY, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::HSTRING;

/// Where the policy that allows software to send Ctrl-Alt-Del is kept.
pub(crate) const SAS_POLICY_KEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";

/// How Mouser runs on this PC, as far as its limitations go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mode {
    /// A portable run, and whether it is elevated; `None` when installed.
    pub(crate) portable: Option<bool>,
    /// Whether Ctrl-Alt-Del goes through the service (the session engine).
    pub(crate) service: bool,
    /// `SoftwareSASGeneration`, when it is set.
    pub(crate) sas_policy: Option<u32>,
}

impl Mode {
    /// This PC: whether this run is `portable` and has the `service`.
    pub(crate) fn of_this_pc(portable: bool, service: bool) -> Mode {
        let elevated =
            || crate::foreground::is_admin(crate::uipi::integrity_level(std::process::id()));
        Mode {
            portable: portable.then(elevated),
            service,
            sas_policy: read_dword(HKEY_LOCAL_MACHINE, SAS_POLICY_KEY, "SoftwareSASGeneration"),
        }
    }
}

/// The DWORD `name` under `root\key`; `None` when it is missing.
pub(crate) fn read_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let read = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(key),
            &HSTRING::from(name),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    read.is_ok().then_some(value)
}

/// The limitations of a run in `mode`.
pub(crate) fn limitations(mode: Mode) -> Vec<Limitation> {
    let secure_attention = match mode {
        Mode {
            portable: Some(false),
            ..
        } => Some(
            "Portable Mouser can't control the login screen, permission prompts, Ctrl-Alt-Del, \
             or applications running as administrator.",
        ),
        Mode {
            portable: Some(true),
            ..
        } => Some(
            "Portable Mouser can't control the login screen, permission prompts or Ctrl-Alt-Del.",
        ),
        // The service sends it, unless the policy keeps software from doing so.
        Mode {
            service: true,
            sas_policy: Some(1 | 3),
            ..
        } => None,
        Mode { service: true, .. } => Some("Ctrl-Alt-Del is blocked by this PC's policy."),
        Mode { service: false, .. } => Some("Ctrl-Alt-Del can't be sent to this PC yet."),
    };
    let mut all = vec![limitation(
        Capability::Inject,
        "Some games and remote-desktop apps ignore Mouser's input on this PC.",
    )];
    all.extend(secure_attention.map(|text| limitation(Capability::SecureAttention, text)));
    all.push(limitation(
        Capability::Install,
        "Windows warns that Mouser comes from an unknown publisher when it is installed or first run.",
    ));
    all
}
