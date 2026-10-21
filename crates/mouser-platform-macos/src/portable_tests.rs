use crate::MacPlatform;
use crate::portable::FileSecrets;
use mouser_platform::{Capability, Platform, SecretStore};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mouser-portable-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn secrets_are_files_under_the_data_dir_that_survive_reopening() {
    let dir = scratch("secrets");
    let mut store = FileSecrets::new(&dir);
    assert_eq!(store.get("desk").unwrap(), None);
    store.set("desk", b"first").unwrap();
    store.set("desk", &[0, 255, 7]).unwrap();
    assert_eq!(
        FileSecrets::new(&dir).get("desk").unwrap(),
        Some(vec![0, 255, 7])
    );
    let file = dir.join("secrets").join("desk");
    assert!(file.is_file());
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    store.delete("desk").unwrap();
    assert_eq!(store.get("desk").unwrap(), None);
    store.delete("desk").unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn portable_platform_uses_the_file_store_and_never_the_keychain() {
    let dir = scratch("platform");
    let platform = MacPlatform::portable(&dir);
    let mut secrets = platform.secrets();
    secrets.set("k", b"v").unwrap();
    assert_eq!(std::fs::read(dir.join("secrets").join("k")).unwrap(), b"v");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn portable_limitations_say_permissions_are_needed_on_each_computer() {
    let info = MacPlatform::portable(scratch("info")).info();
    let text = info
        .limitations
        .iter()
        .find(|l| l.capability == Capability::Capture)
        .unwrap()
        .text
        .clone();
    assert_eq!(
        text,
        "macOS permissions must be granted again on each computer you run Mouser from this drive on."
    );
    // The installed platform keeps its original two limitations.
    assert_eq!(MacPlatform::new().info().limitations.len(), 2);
    assert_eq!(info.limitations.len(), 3);
}
