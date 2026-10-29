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
    let served = crate::support_tests::served_at(&control, mode);
    crate::support_tests::eventually(|| served.exists()).await;
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

/// Run a headless engine until `stop` asks this process to stop, and check it
/// shut down (the control endpoint is removed by `Engine::shutdown`).
async fn shuts_down_on(stop: impl FnOnce()) {
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
    stop();
    task.await.unwrap();
    assert!(!control.exists());
}

/// Sends `signal` to this process.
#[cfg(unix)]
fn kill(signal: &str) {
    let sent = std::process::Command::new("kill")
        .arg(signal)
        .arg(std::process::id().to_string())
        .status()
        .unwrap();
    assert!(sent.success());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_shuts_the_engine_down_before_exit() {
    shuts_down_on(|| kill("-TERM")).await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_shuts_the_engine_down_before_exit() {
    shuts_down_on(|| kill("-INT")).await;
}

/// Set for the copy of the Ctrl-C test that runs on a console of its own.
#[cfg(windows)]
const ON_ITS_OWN_CONSOLE: &str = "MOUSER_TEST_ON_ITS_OWN_CONSOLE";

/// Presses Ctrl-C on this process's console, after checking that nothing
/// else shares that console.
#[cfg(windows)]
fn ctrl_c_on_this_console() {
    use windows_sys::Win32::System::Console::{
        CTRL_C_EVENT, GenerateConsoleCtrlEvent, GetConsoleProcessList, SetConsoleCtrlHandler,
    };
    let mut on_console = [0u32; 8];
    let count = unsafe { GetConsoleProcessList(on_console.as_mut_ptr(), 8) };
    assert_eq!(count, 1, "processes on this console: {on_console:?}");
    // A process can start with Ctrl-C turned off; turn it back on.
    assert_ne!(unsafe { SetConsoleCtrlHandler(None, 0) }, 0);
    assert_ne!(unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) }, 0);
}

/// A copy of this test binary running one test on a console of its own, with
/// no window, writing what it prints to a file. Dropped, it is ended and
/// waited for, should it still be running.
#[cfg(windows)]
struct OwnConsole {
    copy: std::process::Child,
    output: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

#[cfg(windows)]
impl OwnConsole {
    fn run(test: &str) -> Self {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output.txt");
        let file = std::fs::File::create(&output).unwrap();
        let copy = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env(ON_ITS_OWN_CONSOLE, "1")
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        Self {
            copy,
            output,
            _dir: dir,
        }
    }

    /// Waits up to `limit` for the copy to end by itself: how it exited, and
    /// what it printed.
    async fn finish(&mut self, limit: std::time::Duration) -> (std::process::ExitStatus, String) {
        let end = std::time::Instant::now() + limit;
        let status = loop {
            if let Some(status) = self.copy.try_wait().unwrap() {
                break status;
            }
            assert!(
                std::time::Instant::now() < end,
                "the copy ended within {limit:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        (status, std::fs::read_to_string(&self.output).unwrap())
    }
}

#[cfg(windows)]
impl Drop for OwnConsole {
    fn drop(&mut self) {
        let _ = self.copy.kill();
        let _ = self.copy.wait();
    }
}

/// Ctrl-C reaches every process on the console it is pressed on, so pressed
/// from here it would reach the test harness and cargo as well. Instead this
/// test runs a copy of itself on a console of its own, and that copy presses
/// Ctrl-C on its own console, as the unix tests signal their own process. The
/// copy ends by itself, so what it measured for coverage is written whole.
#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_shuts_the_engine_down_before_exit() {
    if std::env::var_os(ON_ITS_OWN_CONSOLE).is_some() {
        return shuts_down_on(ctrl_c_on_this_console).await;
    }
    let mut copy = OwnConsole::run("headless_tests::ctrl_c_shuts_the_engine_down_before_exit");
    let (status, output) = copy.finish(std::time::Duration::from_secs(60)).await;
    assert!(status.success(), "{output}");
    assert!(output.contains("1 passed"), "{output}");
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
    let served = crate::support_tests::served_at(&control, ComputerMode::Service);
    crate::support_tests::eventually(|| served.exists() && secrets_dir(dir.path()).is_dir()).await;
    stop.send(()).unwrap();
    task.await.unwrap();
}
