use super::*;
use mouser_platform::Platform;
use mouser_testkit::VirtualPlatform;

#[test]
fn first_launch_generates_and_stores_the_device_key() {
    let platform = VirtualPlatform::new();
    let mut secrets = platform.secrets();
    assert_eq!(secrets.get(DEVICE_KEY).unwrap(), None);
    let identity = Identity::load_or_create(secrets.as_mut());
    let stored = secrets.get(DEVICE_KEY).unwrap().expect("key stored");
    assert_eq!(stored.len(), 32);
    assert_eq!(identity.seed().as_slice(), stored.as_slice());
}

#[test]
fn later_launches_reuse_the_key() {
    let platform = VirtualPlatform::new();
    let first = Identity::load_or_create(platform.secrets().as_mut());
    let second = Identity::load_or_create(platform.secrets().as_mut());
    assert_eq!(first.computer_id(), second.computer_id());
    assert_eq!(first.public_key(), second.public_key());
}

#[test]
fn different_computers_get_different_identities() {
    let a = Identity::load_or_create(VirtualPlatform::new().secrets().as_mut());
    let b = Identity::load_or_create(VirtualPlatform::new().secrets().as_mut());
    assert_ne!(a.computer_id(), b.computer_id());
}

#[test]
fn computer_id_derives_from_the_public_key() {
    let identity = Identity::from_seed([7; 32]);
    assert_eq!(
        identity.computer_id(),
        ComputerId::from_public_key(&identity.public_key())
    );
    let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]).verifying_key();
    assert_eq!(identity.public_key().0, key.to_bytes());
}
