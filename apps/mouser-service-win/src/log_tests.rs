use mouser_core::logging::events;
use mouser_proto::ids::ComputerId;

use crate::log::open;
use crate::test_dir::TestDir;

const ME: ComputerId = ComputerId([7; 16]);

/// A machine folder whose config turns logging on with `max_log_mb`.
fn machine(logging: bool, max_log_mb: u32) -> TestDir {
    let dir = TestDir::new();
    let mut config = mouser_core::config::load_or_create(dir.path());
    config.diagnostics.logging = logging;
    config.diagnostics.max_log_mb = max_log_mb;
    config.general.computer_name = "Alpha".into();
    mouser_core::config::save(dir.path(), &config);
    dir
}

fn text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_service_log_is_in_the_machine_folder_and_starts_with_its_header() {
    let machine = machine(true, 8);
    let log = open(machine.path(), ME, "HOST");
    log.write(events::service_session(1));
    assert_eq!(log.path(), machine.path().join("logs").join("service.log"));
    let text = text(&log.path());
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].contains(r#""event":"log.header""#), "{text}");
    assert!(lines[0].contains(r#""mode":"service""#), "{text}");
    assert!(lines[0].contains(r#""computer_name":"Alpha""#), "{text}");
    assert!(lines[1].contains(r#""event":"service.session""#), "{text}");
}

#[test]
fn a_computer_without_a_name_of_its_own_goes_by_its_host_name() {
    let machine = TestDir::new();
    let mut config = mouser_core::config::load_or_create(machine.path());
    config.diagnostics.logging = true;
    mouser_core::config::save(machine.path(), &config);
    let log = open(machine.path(), ME, "HOST");
    log.write(events::service_session(1));
    assert!(text(&log.path()).contains(r#""computer_name":"HOST""#));
}

#[test]
fn nothing_is_written_while_logging_is_off() {
    let machine = machine(false, 8);
    let log = open(machine.path(), ME, "HOST");
    log.write(events::service_session(1));
    assert!(!log.path().exists());
}

#[test]
fn the_service_log_rotates_within_the_configured_limit() {
    let machine = machine(true, 1);
    let log = open(machine.path(), ME, "HOST");
    // Each record is ~150 bytes: half a MiB takes a few thousand.
    for n in 0..6000 {
        log.write(events::service_engine_exited(n));
    }
    let rotated = machine.path().join("logs").join("service.log.1");
    assert!(rotated.exists());
    let half = 512 * 1024;
    assert!(std::fs::metadata(&rotated).unwrap().len() <= half);
    assert!(std::fs::metadata(log.path()).unwrap().len() <= half);

    assert!(
        text(&log.path())
            .lines()
            .next()
            .unwrap()
            .contains("log.header")
    );
}

/// The catalog's `service.*` rows are the service's only records.
#[test]
fn the_service_writes_only_service_events() {
    for (name, source) in [
        ("session.rs", include_str!("session.rs")),
        ("sas.rs", include_str!("sas.rs")),
        ("log.rs", include_str!("log.rs")),
        ("launch.rs", include_str!("launch.rs")),
        ("ipc.rs", include_str!("ipc.rs")),
        ("install.rs", include_str!("install.rs")),
        ("service.rs", include_str!("service.rs")),
        ("args.rs", include_str!("args.rs")),
        ("machine.rs", include_str!("machine.rs")),
        ("migrate.rs", include_str!("migrate.rs")),
    ] {
        for call in source.split("events::").skip(1) {
            assert!(
                call.starts_with("service_"),
                "{name}: events::{}",
                &call[..20]
            );
        }
    }
}
