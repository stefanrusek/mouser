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
