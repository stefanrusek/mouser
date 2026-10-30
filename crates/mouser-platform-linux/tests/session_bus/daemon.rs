//! A private `dbus-daemon` (no service activation) standing in for the
//! desktop's session bus.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// The runtime the fake services run on, separate from the one under test.
pub static FAKES: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
});

pub struct Daemon {
    _process: Process,
    pub address: String,
    _dir: tempfile::TempDir,
}

impl Daemon {
    pub fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("bus");
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>"#,
                socket.display()
            ),
        )
        .unwrap();
        // Its own streams, not the test's: a daemon outliving an aborted test
        // binary would otherwise keep cargo's output pipe open for ever.
        let child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--config-file")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be installed to run the session-bus tests");
        let process = ready(
            Process(child),
            &socket,
            Instant::now() + Duration::from_secs(10),
        );
        Daemon {
            _process: process,
            address: format!("unix:path={}", socket.display()),
            _dir: dir,
        }
    }
}

/// A child process that is killed and reaped when dropped, also while a
/// test is failing.
pub struct Process(pub Child);

impl Drop for Process {
    fn drop(&mut self) {
        let killed = self.0.kill().and_then(|()| self.0.wait());
        // A second panic while one is unwinding would abort the process.
        if !std::thread::panicking() {
            killed.unwrap();
        }
    }
}

/// `process` once it listens on `socket`; dropped (killed) if it does not
/// by `deadline`.
fn ready(process: Process, socket: &Path, deadline: Instant) -> Process {
    while std::os::unix::net::UnixStream::connect(socket).is_err() {
        assert!(Instant::now() < deadline, "dbus-daemon did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    process
}

/// Polls `condition` for up to ten seconds.
pub fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A daemon that never gets ready is killed and reaped, not left running with
/// the test's output pipes (cargo would wait for them for ever).
#[test]
fn a_daemon_that_never_gets_ready_leaves_no_process_behind() {
    let child = Command::new("sleep")
        .arg("1000")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let dir = tempfile::tempdir().unwrap();
    let never = dir.path().join("never");
    let waited = std::panic::catch_unwind(|| ready(Process(child), &never, Instant::now()));
    assert!(waited.is_err(), "the readiness wait times out");
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}
