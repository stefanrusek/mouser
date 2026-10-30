use std::path::Path;

use mouser_core::engine_headless::{FileSecrets, SecretStore};
use mouser_core::identity::{DEVICE_KEY, Identity};
use mouser_core::paths::Paths;
use mouser_platform_windows::{DiagLevel, DiagValue};
use mouser_proto::wire::ComputerMode;
use serde_json::Value;

use crate::guard_log::{guard_sink, location};

/// A Mouser folder whose config has logging `on`, named `name`.
fn folder(on: bool, name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.diagnostics.logging = on;
    config.general.computer_name = name.into();
    mouser_core::config::save(dir.path(), &config);
    dir
}

fn secrets(dir: &Path) -> FileSecrets {
    FileSecrets::new(dir.join("secrets"))
}

fn records(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join("logs").join("mouser.log"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// What the platform's guard says when it has put the pointer back.
fn pointer_shown(sink: &crate::diagnostics::Sink) {
    sink(
        DiagLevel::Info,
        "pointer.shown",
        vec![("reason", DiagValue::Str("guard".into()))],
    );
}

#[test]
fn the_session_engines_guard_logs_in_the_machine_folder_and_a_users_in_theirs() {
    let user = Paths {
        dir: "C:\\Users\\a\\AppData\\Roaming\\Mouser".into(),
        portable: false,
    };
    assert_eq!(
        location(true, &user),
        (r"C:\ProgramData\Mouser".into(), ComputerMode::Service)
    );
    assert_eq!(
        location(false, &user),
        (user.dir.clone(), ComputerMode::Installed)
    );
    let portable = Paths {
        dir: "E:\\Mouser\\MouserData".into(),
        portable: true,
    };
    assert_eq!(
        location(false, &portable),
        (portable.dir.clone(), ComputerMode::Portable)
    );
}

#[test]
fn a_guard_that_puts_the_pointer_back_says_so_in_the_log_as_core_writes_it() {
    let dir = folder(true, "Alpha");
    let mut store = secrets(dir.path());
    store.set(DEVICE_KEY, &[9; 32]).unwrap();
    let sink = guard_sink(dir.path(), ComputerMode::Portable, &store, "HOST").unwrap();
    // Nothing is written before the pointer is put back, and another
    // diagnostic is not the guard's to write.
    sink(DiagLevel::Debug, "capture.probe", vec![]);
    assert!(!dir.path().join("logs").exists());
    pointer_shown(&sink);
    let records = records(dir.path());
    assert_eq!(records.len(), 2, "{records:?}");
    let id = Identity::from_seed([9; 32]).computer_id().to_string();
    assert_eq!(records[0]["event"], "log.header");
    assert_eq!(records[0]["fields"]["mode"], "portable");
    assert_eq!(records[0]["fields"]["computer_id"], id);
    assert_eq!(records[1]["event"], "pointer.shown");
    assert_eq!(records[1]["level"], "info");
    assert_eq!(records[1]["computer"]["name"], "Alpha");
    assert_eq!(records[1]["fields"], serde_json::json!({"reason": "guard"}));
}

#[test]
fn a_computer_without_a_name_or_a_key_of_its_own_still_logs() {
    let dir = folder(true, "");
    let mut store = secrets(dir.path());
    // A key that is not one names the computer by zeros, as no key does.
    store.set(DEVICE_KEY, b"short").unwrap();
    pointer_shown(&guard_sink(dir.path(), ComputerMode::Installed, &store, "HOST").unwrap());
    let records = records(dir.path());
    assert_eq!(records[1]["computer"]["name"], "HOST");
    assert_eq!(records[1]["computer"]["id"], "0".repeat(32));
}

#[test]
fn with_logging_off_or_no_config_the_guard_writes_and_creates_nothing() {
    let off = folder(false, "Alpha");
    assert!(
        guard_sink(
            off.path(),
            ComputerMode::Installed,
            &secrets(off.path()),
            "HOST"
        )
        .is_none()
    );
    assert!(!off.path().join("logs").exists());
    let empty = tempfile::tempdir().unwrap();
    assert!(
        guard_sink(
            empty.path(),
            ComputerMode::Installed,
            &secrets(empty.path()),
            "HOST"
        )
        .is_none()
    );
    assert_eq!(std::fs::read_dir(empty.path()).unwrap().count(), 0);
}
