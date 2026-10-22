//! A private `dbus-daemon` (no service activation) standing in for the
//! desktop's session bus.

use std::process::{Child, Command};
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
    child: Child,
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
        let child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--config-file")
            .arg(&config)
            .spawn()
            .expect("dbus-daemon must be installed to run the session-bus tests");
        let deadline = Instant::now() + Duration::from_secs(10);
        while std::os::unix::net::UnixStream::connect(&socket).is_err() {
            assert!(Instant::now() < deadline, "dbus-daemon did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        Daemon {
            child,
            address: format!("unix:path={}", socket.display()),
            _dir: dir,
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

/// Polls `condition` for up to ten seconds.
pub fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
