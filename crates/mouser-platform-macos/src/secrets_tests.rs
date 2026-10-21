use crate::secrets::KeychainStore;
use mouser_platform::SecretStore;

const SERVICE: &str = "com.mouser.app.test";

#[test]
fn set_get_overwrite_delete_round_trip() {
    let name = format!("mouser-secrets-test-{}", std::process::id());
    let mut store = KeychainStore::new(SERVICE);
    assert_eq!(store.get(&name).unwrap(), None);
    store.set(&name, b"first").unwrap();
    assert_eq!(store.get(&name).unwrap(), Some(b"first".to_vec()));
    store.set(&name, &[0, 255, 7]).unwrap();
    assert_eq!(
        KeychainStore::new(SERVICE).get(&name).unwrap(),
        Some(vec![0, 255, 7])
    );
    store.delete(&name).unwrap();
    assert_eq!(store.get(&name).unwrap(), None);
    store.delete(&name).unwrap();
}
