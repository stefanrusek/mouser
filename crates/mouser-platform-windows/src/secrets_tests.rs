use crate::secrets::DpapiStore;
use mouser_platform::SecretStore;

#[test]
fn set_get_overwrite_delete_round_trip_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("Mouser").join("secrets");
    let mut store = DpapiStore::new(secrets.clone());
    assert_eq!(store.get("desk-key").unwrap(), None);
    store.set("desk-key", b"first").unwrap();
    assert_eq!(store.get("desk-key").unwrap(), Some(b"first".to_vec()));
    store.set("desk-key", &[0, 255, 7]).unwrap();
    assert_eq!(
        DpapiStore::new(secrets.clone()).get("desk-key").unwrap(),
        Some(vec![0, 255, 7])
    );
    store.set("empty", &[]).unwrap();
    assert_eq!(store.get("empty").unwrap(), Some(vec![]));
    store.delete("desk-key").unwrap();
    assert_eq!(store.get("desk-key").unwrap(), None);
    store.delete("desk-key").unwrap();
}

/// Core names a desk's key `desk_key/<desk id>`; v0.2.2 panicked writing it
/// ("write secret: path not found") and the poisoned lock took the control
/// server down with it.
#[test]
fn names_with_a_slash_are_one_escaped_file() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("secrets");
    let mut store = DpapiStore::new(secrets.clone());
    let name = format!("desk_key/{}", "03".repeat(16));
    store.set(&name, b"k").unwrap();
    assert!(
        secrets
            .join(format!("desk_key%2F{}", "03".repeat(16)))
            .is_file()
    );
    assert_eq!(
        DpapiStore::new(secrets.clone()).get(&name).unwrap(),
        Some(b"k".to_vec())
    );
    store.delete(&name).unwrap();
    assert_eq!(store.get(&name).unwrap(), None);
    // Names that were already file names keep their files.
    store.set("device_key", b"d").unwrap();
    assert!(secrets.join("device_key").is_file());
}

/// A store that cannot be written answers with an error instead of
/// panicking inside whoever called it.
#[test]
fn a_store_that_cannot_be_written_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the secrets directory should be.
    let blocked = dir.path().join("secrets");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let mut store = DpapiStore::new(blocked.join("inner"));
    assert!(matches!(
        store.set("k", b"v"),
        Err(mouser_platform::Error::Os(_))
    ));
    let mut store = DpapiStore::new(blocked.clone());
    assert!(matches!(
        store.set("k", b"v"),
        Err(mouser_platform::Error::Os(_))
    ));
    // Nothing was stored, and there is nothing to delete.
    assert_eq!(store.get("k").unwrap(), None);
    store.delete("k").unwrap();
    // Something that is there but is not a secret file cannot be read or
    // removed: an error, not "absent".
    let secrets = dir.path().join("ok");
    std::fs::create_dir_all(secrets.join("k")).unwrap();
    let mut store = DpapiStore::new(secrets);
    assert!(matches!(store.get("k"), Err(mouser_platform::Error::Os(_))));
    assert!(matches!(
        store.delete("k"),
        Err(mouser_platform::Error::Os(_))
    ));
}

/// How SDDL writes `sid` as a trustee: some well-known accounts get an alias
/// instead of the SID (this machine's built-in Administrator, RID 500, is
/// `LA`; the CI runner's user is that account).
fn sddl_trustee(sid: &str) -> String {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &windows::core::HSTRING::from(format!("D:(A;;FA;;;{sid})")),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
    }
    .unwrap();
    let mut text = windows::core::PWSTR::null();
    unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            sd,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            None,
        )
    }
    .unwrap();
    let sddl = unsafe { text.to_string() }.unwrap();
    unsafe {
        LocalFree(Some(HLOCAL(text.0.cast())));
        LocalFree(Some(HLOCAL(sd.0)));
    }
    sddl.strip_prefix("D:(A;;FA;;;")
        .and_then(|t| t.strip_suffix(')'))
        .expect(&sddl)
        .to_string()
}

/// The directory's DACL as SDDL, and the current user as SDDL writes them.
pub(crate) fn dacl_sddl(dir: &std::path::Path) -> (String, String) {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let r = unsafe {
        GetNamedSecurityInfoW(
            &windows::core::HSTRING::from(dir.as_os_str()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            &mut sd,
        )
    };
    assert!(r.is_ok(), "{r:?}");
    let mut text = windows::core::PWSTR::null();
    unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            sd,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            None,
        )
    }
    .unwrap();
    let sddl = unsafe { text.to_string() }.unwrap();
    unsafe {
        LocalFree(Some(HLOCAL(text.0.cast())));
        LocalFree(Some(HLOCAL(sd.0)));
    }
    (sddl, sddl_trustee(&crate::secrets::current_user_sid()))
}

/// foundational-notes.md, 2026-10-22: only SYSTEM, Administrators and the
/// installing user may open the secrets directory; nothing is inherited.
#[test]
fn a_new_secrets_directory_is_private_to_system_administrators_and_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join(r"Mouser\secrets");
    DpapiStore::new(secrets.clone())
        .set("desk-key", b"k")
        .unwrap();
    let (sddl, user) = dacl_sddl(&secrets);
    assert!(user.starts_with("S-1-5-21-") || user == "LA", "{user}");
    // P: protected (nothing inherited from ProgramData); AI: set by SetNamedSecurityInfoW.
    assert_eq!(
        sddl,
        format!("D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{user})")
    );
    // The secret file inherits exactly those entries.
    let (file_sddl, _) = dacl_sddl(&secrets.join("desk-key"));
    assert_eq!(
        file_sddl,
        format!("D:AI(A;ID;FA;;;SY)(A;ID;FA;;;BA)(A;ID;FA;;;{user})")
    );
}

#[test]
fn files_hold_dpapi_blobs_not_the_secret() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DpapiStore::new(dir.path().to_path_buf());
    let secret = b"mouser-plaintext-desk-key-0123456789";
    store.set("device-key", secret).unwrap();
    let blob = std::fs::read(dir.path().join("device-key")).unwrap();
    assert!(
        !blob.windows(secret.len()).any(|w| w == secret),
        "secret stored in clear"
    );
    // A DPAPI blob starts with version 1 and the DPAPI provider GUID df9d8cd0-1501-11d1-8c7a-00c04fc297eb.
    assert_eq!(&blob[..4], &[1, 0, 0, 0]);
    assert_eq!(&blob[4..8], &[0xd0, 0x8c, 0x9d, 0xdf]);
    // Then the master-key version and GUID, then the flags: CRYPTPROTECT_LOCAL_MACHINE
    // (machine scope, readable by mouser-service before login) is recorded there.
    let flags = u32::from_le_bytes(blob[40..44].try_into().unwrap());
    assert_ne!(flags & 0x4, 0, "not machine scope: flags {flags:#x}");
}

/// A blob DPAPI cannot open (corrupt, or not one) is an error the caller can
/// report: the installer copying the identity says so (research V12a).
#[test]
fn a_secret_that_is_not_a_dpapi_blob_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("device_key"), b"not a blob").unwrap();
    let store = DpapiStore::new(dir.path().to_path_buf());
    assert!(matches!(
        store.get("device_key"),
        Err(mouser_platform::Error::Os(_))
    ));
}

#[test]
fn the_machine_store_of_a_folder_is_open_to_other_crates() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = crate::WinPlatform::secret_store(dir.path().join("secrets"));
    store.set("device_key", b"key").unwrap();
    assert_eq!(
        DpapiStore::new(dir.path().join("secrets"))
            .get("device_key")
            .unwrap(),
        Some(b"key".to_vec())
    );
}
