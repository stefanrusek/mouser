use crate::portable::*;
use crate::secrets_tests::dacl_sddl;
use crate::{Settings, WinPlatform};
use mouser_platform::{Capability, Error, Platform};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegDeleteTreeW, RegOpenKeyExW,
};
use windows::core::HSTRING;

fn settings(name: &str, root: &std::path::Path) -> Settings {
    let key = format!(
        r"Software\MouserTest\portable-{name}-{}",
        std::process::id()
    );
    Settings {
        // Where an installed run would keep its DPAPI secrets.
        secrets_dir: root.join("machine-secrets"),
        run_key: format!(r"{key}\Run"),
        approved_key: format!(r"{key}\StartupApproved\Run"),
        exe: r"E:\Mouser\Mouser.exe".into(),
    }
}

fn key_exists(path: &str) -> bool {
    let mut key = HKEY::default();
    let r = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            KEY_READ,
            &mut key,
        )
    };
    if r == ERROR_SUCCESS {
        let _ = unsafe { RegCloseKey(key) };
    }
    r == ERROR_SUCCESS
}

fn text_of(p: &WinPlatform, capability: Capability) -> Vec<String> {
    p.info()
        .limitations
        .into_iter()
        .filter(|l| l.capability == capability)
        .map(|l| l.text)
        .collect()
}

#[test]
fn secrets_are_plain_files_under_the_drives_mouser_data() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("MouserData");
    let p = WinPlatform::portable_with_settings(settings("secrets", root.path()), &data);
    let mut secrets = p.secrets();
    assert_eq!(secrets.get("desk-key").unwrap(), None);
    secrets.set("desk-key", b"abc").unwrap();
    secrets.set("desk-key", b"abcdef").unwrap();
    // Not a DPAPI blob: that would only open on this computer.
    assert_eq!(
        std::fs::read(data.join(r"secrets\desk-key")).unwrap(),
        b"abcdef"
    );
    assert_eq!(
        p.secrets().get("desk-key").unwrap(),
        Some(b"abcdef".to_vec())
    );
    secrets.delete("desk-key").unwrap();
    secrets.delete("desk-key").unwrap();
    assert_eq!(secrets.get("desk-key").unwrap(), None);
    assert!(
        !root.path().join("machine-secrets").exists(),
        "nothing was written to the computer"
    );
}

#[test]
fn secret_names_that_are_not_file_names_are_escaped() {
    assert_eq!(file_name("device_key"), "device_key");
    assert_eq!(
        file_name(r#"a/b\c:d*e?f"g<h>i|j%k"#),
        "a%2Fb%5Cc%3Ad%2Ae%3Ff%22g%3Ch%3Ei%7Cj%25k"
    );
    let root = tempfile::tempdir().unwrap();
    let p = WinPlatform::portable_with_settings(settings("names", root.path()), root.path());
    let mut secrets = p.secrets();
    secrets.set("desk/key:1", b"v").unwrap();
    assert!(root.path().join(r"secrets\desk%2Fkey%3A1").is_file());
    assert_eq!(secrets.get("desk/key:1").unwrap(), Some(b"v".to_vec()));
}

#[test]
fn a_file_store_that_cannot_read_write_or_delete_says_so() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("MouserData");
    let p = WinPlatform::portable_with_settings(settings("errors", root.path()), &data);
    let mut secrets = p.secrets();
    // A folder where the secret's file should be.
    std::fs::create_dir(data.join(r"secrets\in-the-way")).unwrap();
    assert!(matches!(secrets.get("in-the-way"), Err(Error::Os(_))));
    assert!(matches!(secrets.delete("in-the-way"), Err(Error::Os(_))));
    assert!(matches!(secrets.set("in-the-way", b"x"), Err(Error::Os(_))));
    // A file where the secrets folder should be.
    std::fs::remove_dir_all(data.join("secrets")).unwrap();
    std::fs::write(data.join("secrets"), b"not a folder").unwrap();
    assert!(matches!(secrets.set("x", b"y"), Err(Error::Os(_))));
}

#[test]
fn a_portable_run_writes_nothing_to_the_registry_and_cannot_autostart() {
    let root = tempfile::tempdir().unwrap();
    let s = settings("registry", root.path());
    let (run, approved) = (s.run_key.clone(), s.approved_key.clone());
    let p = WinPlatform::portable_with_settings(s, root.path());
    let mut autostart = p.autostart();
    assert!(!autostart.enabled());
    assert_eq!(autostart.set(true), Err(Error::Unsupported));
    assert!(!autostart.enabled());
    p.secrets().set("k", b"v").unwrap();
    assert!(!key_exists(&run) && !key_exists(&approved));

    // The same settings on an installed run do reach the registry, so the
    // check above would have noticed.
    let installed = WinPlatform::with_settings(settings("registry", root.path()));
    installed.autostart().set(true).unwrap();
    assert!(key_exists(&run));
    installed.autostart().set(false).unwrap();
    let root_key = run.split('\\').take(3).collect::<Vec<_>>().join("\\");
    let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(root_key)) };
}

#[test]
fn a_portable_run_says_what_it_cannot_control() {
    let root = tempfile::tempdir().unwrap();
    let portable =
        WinPlatform::portable_with_settings(settings("limits", root.path()), root.path());
    let said = text_of(&portable, Capability::SecureAttention);
    assert!(said.contains(&WINDOWS_LIMITATION.to_string()), "{said:?}");
    assert_eq!(
        WINDOWS_LIMITATION,
        "Portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator."
    );
    // The limitations every Windows run has stay.
    assert!(said.contains(&"Ctrl-Alt-Del can't be sent to this PC yet.".to_string()));
    assert_eq!(text_of(&portable, Capability::Inject).len(), 1);

    let installed = WinPlatform::with_settings(settings("limits", root.path()));
    let installed_said = text_of(&installed, Capability::SecureAttention);
    assert!(!installed_said.contains(&WINDOWS_LIMITATION.to_string()));
}

/// foundational-notes.md: on a volume with file permissions only the current
/// user and SYSTEM may open the secrets; nothing is inherited and, since the
/// drive moves between computers, Administrators are not added.
#[test]
fn on_a_volume_with_permissions_the_secrets_folder_is_private_to_the_user_and_system() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("MouserData");
    let p = WinPlatform::portable_with_settings(settings("acl", root.path()), &data);
    let secrets = data.join("secrets");
    assert!(
        secrets.is_dir(),
        "made at once, for a store that is not ours"
    );
    let (sddl, user) = dacl_sddl(&secrets);
    assert_eq!(sddl, format!("D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;{user})"));
    assert!(!sddl.contains(";BA)"), "{sddl}");
    // Whoever writes the secrets (the platform's store, or the engine's own
    // file store) gets files that inherit exactly that.
    p.secrets().set("device_key", b"k").unwrap();
    std::fs::write(secrets.join("written-by-someone-else"), b"k").unwrap();
    for name in ["device_key", "written-by-someone-else"] {
        let (file, _) = dacl_sddl(&secrets.join(name));
        assert_eq!(
            file,
            format!("D:AI(A;ID;FA;;;SY)(A;ID;FA;;;{user})"),
            "{name}"
        );
    }
    assert!(text_of(&p, Capability::Capture).is_empty());
}

#[test]
fn secrets_already_on_the_drive_are_protected_when_a_portable_run_starts() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("MouserData");
    std::fs::create_dir_all(data.join("secrets")).unwrap();
    std::fs::write(data.join(r"secrets\device_key"), b"k").unwrap();
    let file = data.join(r"secrets\device_key");
    let (before, user) = dacl_sddl(&file);
    let private = format!("D:AI(A;ID;FA;;;SY)(A;ID;FA;;;{user})");
    assert_ne!(before, private);
    let _p = WinPlatform::portable_with_settings(settings("existing", root.path()), &data);
    let (after, _) = dacl_sddl(&file);
    assert_eq!(after, private);
    // Starting again changes nothing.
    let _again = WinPlatform::portable_with_settings(settings("existing", root.path()), &data);
    assert_eq!(dacl_sddl(&file).0, after);
}

#[test]
fn on_a_volume_without_permissions_nothing_is_set_and_the_app_says_so() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("MouserData");
    // What FAT32 and exFAT report: no persistent ACLs.
    let fat: &dyn Fn(&std::path::Path) -> mouser_platform::Result<u32> = &|_| Ok(0x0000_0003);
    assert_eq!(prepare_with(&data, fat), Protection::Unprotected);
    let (sddl, _) = dacl_sddl(&data.join("secrets"));
    assert!(!sddl.starts_with("D:P"), "inheritance left alone: {sddl}");

    let mut p = WinPlatform::portable_with_settings(settings("fat", root.path()), root.path());
    p.portable = Some(Portable {
        data_dir: data,
        protection: Protection::Unprotected,
    });
    assert_eq!(
        text_of(&p, Capability::Capture),
        [
            "Secrets on this drive are not protected by file permissions; anyone with the drive can read them."
        ]
    );
    assert_eq!(UNPROTECTED, text_of(&p, Capability::Capture)[0]);
}

#[test]
fn a_volume_that_cannot_be_asked_is_treated_as_having_no_permissions() {
    let root = tempfile::tempdir().unwrap();
    let broken: &dyn Fn(&std::path::Path) -> mouser_platform::Result<u32> =
        &|_| Err(Error::Os("GetVolumeInformationW: denied".into()));
    assert_eq!(
        prepare_with(&root.path().join("MouserData"), broken),
        Protection::Unprotected
    );
}

#[test]
fn this_machines_volume_reports_permissions_and_the_sddl_names_the_user() {
    let root = tempfile::tempdir().unwrap();
    let flags = volume_flags(root.path()).unwrap();
    assert_ne!(
        flags & 0x8,
        0,
        "the temp volume has file permissions: {flags:#x}"
    );
    assert!(private_sddl().starts_with("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;S-1-5-21-"));
    assert!(volume_flags(&root.path().join("not-yet-there")).is_ok());
}

#[test]
fn the_system_portable_platform_is_built_from_this_machine() {
    let root = tempfile::tempdir().unwrap();
    let p = WinPlatform::portable(root.path().join("MouserData"));
    assert!(p.autostart().set(true).is_err());
    assert!(root.path().join(r"MouserData\secrets").is_dir());
}
