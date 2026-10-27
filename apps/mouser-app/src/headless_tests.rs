use std::sync::Arc;

use mouser_platform::Os;
use mouser_proto::wire::ComputerMode;
use mouser_testkit::VirtualPlatform;

use crate::headless::{check_supported, is_headless, machine_dir, run, secrets_dir};
use crate::support_tests::{control_path_in, free_port};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

#[test]
fn the_headless_flag_selects_headless_mode() {
    assert!(is_headless(&args(&["mouser", "--headless"])));
    assert!(is_headless(&args(&["mouser", "--other", "--headless"])));
}

#[test]
fn without_the_flag_the_normal_app_runs() {
    assert!(!is_headless(&args(&["mouser"])));
    assert!(!is_headless(&args(&["mouser", "headless"])));
    assert!(!is_headless(&[]));
}

#[test]
fn headless_is_only_supported_on_macos() {
    assert_eq!(check_supported(Os::MacOs), Ok(()));
    let refusal = "headless mode is only supported on macOS in this version";
    assert_eq!(check_supported(Os::Linux), Err(refusal));
    assert_eq!(check_supported(Os::Windows), Err(refusal));
}

#[test]
fn each_os_has_its_own_machine_directory() {
    assert_eq!(
        machine_dir(Os::MacOs),
        std::path::PathBuf::from("/Library/Application Support/Mouser")
    );
    assert_eq!(
        machine_dir(Os::Windows),
        std::path::PathBuf::from(r"C:\ProgramData\Mouser")
    );
    assert_eq!(
        machine_dir(Os::Linux),
        std::path::PathBuf::from("/var/lib/mouser")
    );
}

#[test]
fn machine_secrets_live_in_a_folder_of_the_machine_directory() {
    assert_eq!(
        secrets_dir(std::path::Path::new("/m")),
        std::path::PathBuf::from("/m/secrets")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn headless_runs_the_engine_on_machine_scope_secrets_until_signalled() {
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    mouser_core::config::save(dir.path(), &config);
    let control = control_path_in(&dir);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let machine = dir.path().to_path_buf();
    let task = tokio::spawn(run(
        Arc::new(VirtualPlatform::new()),
        machine.clone(),
        control.clone(),
        "Login Mac".into(),
        "1.2.3".into(),
        ComputerMode::Headless,
        async {
            stopped.await.unwrap();
        },
    ));
    crate::support_tests::eventually(|| control.exists() && secrets_dir(&machine).is_dir()).await;
    // The device key went to the machine-scope store, not the platform's.
    assert!(std::fs::read_dir(secrets_dir(&machine)).unwrap().count() > 0);
    assert!(!task.is_finished());
    stop.send(()).unwrap();
    task.await.unwrap();
    assert!(!control.exists());
}

/// Run a headless engine with logging on until `check` has looked at it.
async fn headless_logging(mode: ComputerMode, check: impl FnOnce(&std::path::Path)) {
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    config.diagnostics.logging = true;
    mouser_core::config::save(dir.path(), &config);
    let control = control_path_in(&dir);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(run(
        Arc::new(VirtualPlatform::new()),
        dir.path().to_path_buf(),
        control.clone(),
        "Login Mac".into(),
        "1.2.3".into(),
        mode,
        async {
            stopped.await.unwrap();
        },
    ));
    crate::support_tests::eventually(|| control.exists()).await;
    check(dir.path());
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_headless_engine_is_started_with_the_mode_it_was_given() {
    headless_logging(ComputerMode::Service, |dir| {
        let records = crate::support_tests::log_records(dir);
        assert_eq!(records[0]["event"], "log.header");
        assert_eq!(records[0]["fields"]["mode"], "service");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn headless_start_installs_the_panic_hook_on_the_engines_log() {
    headless_logging(ComputerMode::Headless, |dir| {
        crate::support_tests::panic_on_thread("headless-t231", "headless went wrong");
        let records = crate::support_tests::log_records(dir);
        let fault = records
            .iter()
            .find(|r| r["event"] == "fault" && r["fields"]["message"] == "headless went wrong")
            .unwrap_or_else(|| panic!("a fault record: {records:?}"));
        assert_eq!(fault["fields"]["where"], "headless-t231");
    })
    .await;
}
