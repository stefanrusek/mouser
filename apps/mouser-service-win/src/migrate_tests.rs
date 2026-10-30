use mouser_core::engine_headless::{FileSecrets, SecretStore};
use mouser_core::state::{FILE, LocalState};

use crate::migrate::migrate;
use crate::test_dir::TestDir;

/// The installed Mouser's store: one folder for the user's Mouser and the
/// session engine alike.
fn store_in(machine: &TestDir) -> impl Fn() -> Box<dyn SecretStore> + '_ {
    move || Box::new(FileSecrets::new(machine.path().join("secrets")))
}

#[test]
fn the_installing_users_desk_membership_moves_to_the_machine_folder() {
    let (user, machine) = (TestDir::new(), TestDir::new());
    store_in(&machine)().set("device_key", b"key").unwrap();
    let state = LocalState::default();
    state.save(&user.path().join(FILE)).unwrap();
    migrate(user.path(), machine.path(), &store_in(&machine)).unwrap();
    assert_eq!(LocalState::load(&machine.path().join(FILE)), state);
    assert_eq!(
        store_in(&machine)().get("device_key").unwrap(),
        Some(b"key".to_vec())
    );
}

#[test]
fn an_identity_that_is_not_there_is_reported_and_nothing_is_copied() {
    let (user, machine) = (TestDir::new(), TestDir::new());
    assert_eq!(
        migrate(user.path(), machine.path(), &store_in(&machine)),
        Err("device_key is not in the user's store".into())
    );
    assert!(!machine.path().join(FILE).exists());
}
