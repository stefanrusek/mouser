use std::sync::Arc;

use mouser_core::paths::Paths;
use mouser_platform::Os;
use mouser_proto::wire::ComputerMode;
use mouser_testkit::VirtualPlatform;

use crate::headless::{data_dir, is_headless, machine_dir, params, run, secrets_dir};
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

fn user_folder(portable: bool) -> Paths {
    Paths {
        dir: std::path::PathBuf::from("/home/someone/.config/mouser"),
        portable,
    }
}

#[test]
fn headless_no_longer_refuses_on_windows_and_linux_and_uses_the_users_folder() {
    for os in [Os::Windows, Os::Linux] {
        assert_eq!(
            data_dir(os, ComputerMode::Headless, &user_folder(false)),
            user_folder(false).dir
        );
    }
}

#[test]
fn portable_headless_uses_the_mouser_data_folder() {
    let portable = Paths {
        dir: std::path::PathBuf::from("/drive/MouserData"),
        portable: true,
    };
    for os in [Os::Windows, Os::Linux] {
        assert_eq!(
            data_dir(os, ComputerMode::Headless, &portable),
            portable.dir
        );
    }
}

#[test]
fn the_machine_service_uses_the_machine_directory() {
    assert_eq!(
        data_dir(Os::Linux, ComputerMode::Service, &user_folder(false)),
        std::path::PathBuf::from("/var/lib/mouser")
    );
    assert_eq!(
        data_dir(Os::Windows, ComputerMode::Service, &user_folder(false)),
        std::path::PathBuf::from(r"C:\ProgramData\Mouser")
    );
}

#[test]
fn the_macos_login_agent_keeps_using_the_machine_directory() {
    assert_eq!(
        data_dir(Os::MacOs, ComputerMode::Headless, &user_folder(false)),
        machine_dir(Os::MacOs)
    );
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
        params(
            Arc::new(VirtualPlatform::new()),
            Os::MacOs,
            machine.clone(),
            control.clone(),
            "Login Mac".into(),
            "1.2.3".into(),
            ComputerMode::Headless,
        ),
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
        params(
            Arc::new(VirtualPlatform::new()),
            Os::MacOs,
            dir.path().to_path_buf(),
            control.clone(),
            "Login Mac".into(),
            "1.2.3".into(),
            mode,
        ),
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

/// Run a headless engine until `signal` is sent to this process, and check it
/// shut down (the control endpoint is removed by `Engine::shutdown`).
#[cfg(unix)]
async fn shuts_down_on(signal: &str) {
    use crate::headless::stop_signal;
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    mouser_core::config::save(dir.path(), &config);
    let control = control_path_in(&dir);
    let task = tokio::spawn(run(
        params(
            Arc::new(VirtualPlatform::new()),
            Os::MacOs,
            dir.path().to_path_buf(),
            control.clone(),
            "Signalled".into(),
            "1.2.3".into(),
            ComputerMode::Headless,
        ),
        stop_signal(),
    ));
    crate::support_tests::eventually(|| control.exists()).await;
    let sent = std::process::Command::new("kill")
        .arg(signal)
        .arg(std::process::id().to_string())
        .status()
        .unwrap();
    assert!(sent.success());
    task.await.unwrap();
    assert!(!control.exists());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_shuts_the_engine_down_before_exit() {
    shuts_down_on("-TERM").await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_shuts_the_engine_down_before_exit() {
    shuts_down_on("-INT").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn user_scope_headless_keeps_its_secrets_in_the_platforms_store() {
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    mouser_core::config::save(dir.path(), &config);
    let control = control_path_in(&dir);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let platform = VirtualPlatform::new();
    let task = tokio::spawn(run(
        params(
            Arc::new(platform.clone()),
            Os::Linux,
            dir.path().to_path_buf(),
            control.clone(),
            "Linux box".into(),
            "1.2.3".into(),
            ComputerMode::Headless,
        ),
        async {
            stopped.await.unwrap();
        },
    ));
    crate::support_tests::eventually(|| control.exists()).await;
    assert!(!secrets_dir(dir.path()).exists());
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_machine_service_keeps_its_secrets_in_the_machine_directory() {
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    mouser_core::config::save(dir.path(), &config);
    let control = control_path_in(&dir);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(run(
        params(
            Arc::new(VirtualPlatform::new()),
            Os::Linux,
            dir.path().to_path_buf(),
            control.clone(),
            "Linux service".into(),
            "1.2.3".into(),
            ComputerMode::Service,
        ),
        async {
            stopped.await.unwrap();
        },
    ));
    crate::support_tests::eventually(|| control.exists() && secrets_dir(dir.path()).is_dir()).await;
    stop.send(()).unwrap();
    task.await.unwrap();
}
