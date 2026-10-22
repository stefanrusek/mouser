use crate::secrets::{FileSecrets, config_dir, open};
use mouser_platform::SecretStore;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn config_dir_prefers_xdg_config_home_then_home() {
    assert_eq!(
        config_dir(&env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")])),
        std::path::Path::new("/x")
    );
    assert_eq!(
        config_dir(&env(&[("HOME", "/h")])),
        std::path::Path::new("/h/.config")
    );
}

#[test]
fn file_store_round_trips_in_a_0600_file() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FileSecrets {
        dir: root.path().join("secrets"),
    };
    assert_eq!(store.get("device-key").unwrap(), None);
    store.set("device-key", b"first").unwrap();
    assert_eq!(store.get("device-key").unwrap(), Some(b"first".to_vec()));
    store.set("device-key", &[0, 255, 7]).unwrap();
    let file = root.path().join("secrets/device-key");
    assert_eq!(std::fs::read(&file).unwrap(), [0, 255, 7]);
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    store.delete("device-key").unwrap();
    assert_eq!(store.get("device-key").unwrap(), None);
    store.delete("device-key").unwrap();
}

#[test]
fn without_a_secret_service_the_store_is_a_file_in_the_config_dir() {
    let home = tempfile::tempdir().unwrap();
    let env = env(&[("XDG_CONFIG_HOME", home.path().to_str().unwrap())]);
    let mut store = open(&env);
    store.set("desk-key", b"abc").unwrap();
    assert_eq!(
        std::fs::read(home.path().join("mouser/secrets/desk-key")).unwrap(),
        b"abc"
    );
    let mut reopened = open(&env);
    assert_eq!(reopened.get("desk-key").unwrap(), Some(b"abc".to_vec()));
    reopened.delete("desk-key").unwrap();
    assert_eq!(reopened.get("desk-key").unwrap(), None);
}

#[test]
fn file_store_accepts_names_with_slashes_and_keeps_them_distinct() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FileSecrets {
        dir: root.path().join("secrets"),
    };
    store.set("desk_key/ab12", b"one").unwrap();
    store.set("desk_key%2Fab12", b"two").unwrap();
    assert_eq!(store.get("desk_key/ab12").unwrap(), Some(b"one".to_vec()));
    assert_eq!(store.get("desk_key%2Fab12").unwrap(), Some(b"two".to_vec()));
    assert!(root.path().join("secrets/desk_key%2Fab12").is_file());
    assert!(root.path().join("secrets/desk_key%252Fab12").is_file());
    store.delete("desk_key/ab12").unwrap();
    assert_eq!(store.get("desk_key/ab12").unwrap(), None);
    assert_eq!(store.get("desk_key%2Fab12").unwrap(), Some(b"two".to_vec()));
}
