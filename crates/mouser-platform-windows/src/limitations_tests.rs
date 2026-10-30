use crate::limitations::*;
use mouser_platform::{Capability, Limitation};

const NOT_ELEVATED: &str = "Portable Mouser can't control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.";
const ELEVATED: &str =
    "Portable Mouser can't control the login screen, permission prompts or Ctrl-Alt-Del.";
const BLOCKED: &str = "Ctrl-Alt-Del is blocked by this PC's policy.";
const NOT_YET: &str = "Ctrl-Alt-Del can't be sent to this PC yet.";
const UNKNOWN_PUBLISHER: &str =
    "Windows warns that Mouser comes from an unknown publisher when it is installed or first run.";
const GAMES: &str = "Some games and remote-desktop apps ignore Mouser's input on this PC.";

fn of(all: &[Limitation], capability: Capability) -> Vec<&str> {
    all.iter()
        .filter(|l| l.capability == capability)
        .map(|l| l.text.as_str())
        .collect()
}

fn installed(service: bool, sas_policy: Option<u32>) -> Mode {
    Mode {
        portable: None,
        service,
        sas_policy,
    }
}

#[test]
fn a_portable_run_names_what_it_cannot_control_by_whether_it_is_elevated() {
    for (elevated, text) in [(false, NOT_ELEVATED), (true, ELEVATED)] {
        let all = limitations(Mode {
            portable: Some(elevated),
            service: false,
            sas_policy: Some(1),
        });
        assert_eq!(of(&all, Capability::SecureAttention), [text]);
    }
}

#[test]
fn ctrl_alt_del_through_the_service_is_blocked_only_by_a_policy_other_than_1_or_3() {
    for policy in [Some(1), Some(3)] {
        let all = limitations(installed(true, policy));
        assert!(
            of(&all, Capability::SecureAttention).is_empty(),
            "{policy:?}"
        );
    }
    for policy in [Some(0), Some(2), None] {
        let all = limitations(installed(true, policy));
        assert_eq!(
            of(&all, Capability::SecureAttention),
            [BLOCKED],
            "{policy:?}"
        );
    }
}

#[test]
fn without_the_service_ctrl_alt_del_cannot_be_sent_yet() {
    let all = limitations(installed(false, Some(1)));
    assert_eq!(of(&all, Capability::SecureAttention), [NOT_YET]);
}

#[test]
fn every_build_warns_of_the_unknown_publisher_and_of_games_that_ignore_input() {
    for mode in [
        installed(true, Some(1)),
        installed(false, None),
        Mode {
            portable: Some(false),
            service: false,
            sas_policy: None,
        },
    ] {
        let all = limitations(mode);
        assert_eq!(of(&all, Capability::Install), [UNKNOWN_PUBLISHER]);
        assert_eq!(of(&all, Capability::Inject), [GAMES]);
    }
}

#[test]
fn a_policy_value_is_read_as_a_dword_and_a_missing_one_as_none() {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_DWORD, RegDeleteTreeW, RegSetKeyValueW,
    };
    use windows::core::HSTRING;
    let key = format!(r"Software\MouserTest\limitations-{}", std::process::id());
    let path = HSTRING::from(key.as_str());
    let value = 3u32.to_le_bytes();
    let set = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &path,
            windows::core::w!("SoftwareSASGeneration"),
            REG_DWORD.0,
            Some(value.as_ptr().cast()),
            value.len() as u32,
        )
    };
    assert!(set.is_ok(), "{set:?}");
    let read = read_dword(HKEY_CURRENT_USER, &key, "SoftwareSASGeneration");
    let missing = read_dword(HKEY_CURRENT_USER, &key, "NoSuchValue");
    let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &path) };
    assert_eq!(read, Some(3));
    assert_eq!(missing, None);
}

#[test]
fn this_pcs_mode_is_read_without_error() {
    // Whatever this PC's policy is, it is read; this process is not portable.
    let mode = Mode::of_this_pc(false, false);
    assert_eq!(mode.portable, None);
    assert!(!mode.service);
    assert_eq!(
        mode.sas_policy,
        read_dword(
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            SAS_POLICY_KEY,
            "SoftwareSASGeneration"
        )
    );
    let portable = Mode::of_this_pc(true, false);
    assert_eq!(
        portable.portable,
        Some(crate::foreground::is_admin(crate::uipi::integrity_level(
            std::process::id()
        )))
    );
}
