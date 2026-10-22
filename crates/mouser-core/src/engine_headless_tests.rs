use std::path::Path;

use mouser_platform::{Platform, SecretStore};
use mouser_testkit::{Injected, VirtualPlatform};

use crate::engine_headless::FileSecrets;
use crate::identity::DEVICE_KEY;
use crate::state::desk_key_name;
use crate::testing::*;

/// A machine-scope directory for a headless engine: its config (a free
/// port) and the secrets directory beside it.
fn machine_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    crate::config::save(dir.path(), &config);
    dir
}

fn secrets_in(dir: &Path) -> FileSecrets {
    FileSecrets::new(dir.join("secrets"))
}

#[test]
fn file_secrets_keep_values_in_private_files_that_outlive_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileSecrets::new(dir.path().join("nested").join("secrets"));
    assert_eq!(store.get("device_key").unwrap(), None);
    store.set("desk_key/abc", b"one").unwrap();
    store.set("desk_key/abc", b"two").unwrap();
    store.set("device_key", b"dev").unwrap();
    assert_eq!(store.get("desk_key/abc").unwrap(), Some(b"two".to_vec()));
    // A new store over the same directory (the next boot) sees them.
    let again = FileSecrets::new(dir.path().join("nested").join("secrets"));
    assert_eq!(again.get("device_key").unwrap(), Some(b"dev".to_vec()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for entry in std::fs::read_dir(dir.path().join("nested").join("secrets")).unwrap() {
            let mode = entry.unwrap().metadata().unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
    store.delete("desk_key/abc").unwrap();
    assert_eq!(store.get("desk_key/abc").unwrap(), None);
    // Deleting what is not there is fine.
    store.delete("desk_key/abc").unwrap();
}

#[test]
fn file_secrets_report_what_the_disk_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = FileSecrets::new(dir.path().to_path_buf());
    // A directory where the secret's file should be.
    std::fs::create_dir(dir.path().join("taken")).unwrap();
    let os = |r: mouser_platform::Result<()>| matches!(r, Err(mouser_platform::Error::Os(_)));
    assert!(matches!(
        store.get("taken"),
        Err(mouser_platform::Error::Os(_))
    ));
    assert!(os(store.set("taken", b"x")));
    assert!(os(store.delete("taken")));
    // A file where the secrets directory should be.
    std::fs::write(dir.path().join("file"), b"").unwrap();
    let mut store = FileSecrets::new(dir.path().join("file"));
    assert!(os(store.set("a", b"x")));
}

#[tokio::test]
async fn a_headless_engine_loads_the_desk_from_machine_scope_secrets_joins_it_and_accepts_input() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let beta = nodes.pop().unwrap();
    let alpha = nodes.pop().unwrap();
    let beta_id = beta.engine.computer_id();
    // The privileged step run in the user's session: Beta's identity, desk
    // key and desk membership are copied to the machine.
    let machine = machine_dir();
    let secrets_dir = machine.path().join("secrets");
    beta.engine
        .export_machine_scope(machine.path(), &mut FileSecrets::new(secrets_dir.clone()));
    beta.engine.shutdown().await;
    // The login window: no user, no user keychain, a platform that has
    // never heard of the desk.
    let login_window = VirtualPlatform::new();
    let headless = lab
        .boot_with("Beta", login_window.clone(), machine, |params| {
            params.secrets = Some(Box::new(FileSecrets::new(secrets_dir)));
        })
        .await;
    // It is the same computer, back on the desk without any action.
    assert_eq!(headless.engine.computer_id(), beta_id);
    let nodes = vec![alpha, headless];
    ready(&nodes).await;
    // The shared mouse and keyboard act on it.
    push_right(&nodes[0]);
    injected_where(&login_window, "the pointer arriving", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    nodes[0]
        .platform
        .send_input(mouser_platform::LocalInput::Key {
            usage: 0x0007_0004,
            down: true,
        });
    injected_where(&login_window, "a key", |i| {
        matches!(
            i,
            Injected::Key {
                usage: 0x0007_0004,
                down: true
            }
        )
    })
    .await;
    // Nothing was written to a user store.
    assert_eq!(login_window.secrets().get(DEVICE_KEY).unwrap(), None);
}

#[tokio::test]
async fn exporting_before_joining_a_desk_copies_the_identity_alone() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let machine = tempfile::tempdir().unwrap();
    let mut store = secrets_in(machine.path());
    node.engine.export_machine_scope(machine.path(), &mut store);
    let key = node.platform.secrets().get(DEVICE_KEY).unwrap().unwrap();
    assert_eq!(store.get(DEVICE_KEY).unwrap(), Some(key));
    assert!(machine.path().join(crate::state::FILE).exists());
    assert_eq!(
        std::fs::read_dir(machine.path().join("secrets"))
            .unwrap()
            .count(),
        1
    );
}

#[tokio::test]
async fn exporting_copies_the_desk_key() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let machine = tempfile::tempdir().unwrap();
    let mut store = secrets_in(machine.path());
    nodes[0]
        .engine
        .export_machine_scope(machine.path(), &mut store);
    let desk = nodes[0]
        .engine
        .ctx
        .core
        .lock()
        .unwrap()
        .state
        .desk
        .as_ref()
        .unwrap()
        .desk_id;
    let key = nodes[0]
        .platform
        .secrets()
        .get(&desk_key_name(&desk))
        .unwrap()
        .unwrap();
    assert_eq!(store.get(&desk_key_name(&desk)).unwrap(), Some(key));
}
