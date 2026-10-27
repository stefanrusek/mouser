//! Shared helpers for the unit tests: a started host on a virtual platform,
//! a mock Tauri app, and fake tray and notifier ports.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mouser_platform::{
    Autostart, Clipboard, FileDrag, InputCapture, InputInject, LocalDisplay, NetInterface,
    PermissionKind, PermissionState, Platform, PlatformInfo, SecretStore, SecureInputHolder,
    WatchHandle,
};
use mouser_proto::control::{Method, Status};
use mouser_testkit::VirtualPlatform;
use tauri::Manager;
use tauri::test::MockRuntime;

use crate::host::Host;
use crate::notify::Notifier;
use crate::tray::{TrayPort, TrayView};

/// The panic hook is process-wide: tests that start the app or a headless
/// engine (which install it) or panic on purpose take turns.
static HOOKS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn one_at_a_time() -> tokio::sync::MutexGuard<'static, ()> {
    HOOKS.lock().await
}

/// Every JSON record of the diagnostic log under `config_dir`.
pub(crate) fn log_records(config_dir: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(config_dir.join("logs").join("mouser.log"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("log lines are JSON"))
        .collect()
}

/// Panic on a thread called `name` with `message` and wait for it to die.
pub(crate) fn panic_on_thread(name: &str, message: &'static str) {
    let died = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || panic!("{message}"))
        .unwrap()
        .join();
    assert!(died.is_err());
}

pub struct Fixture {
    pub host: Host,
    pub platform: VirtualPlatform,
    pub _dir: tempfile::TempDir,
}

/// The port a default-configured engine listens on. Tests that start an
/// engine without choosing a port take it, so a probe that finds it free is
/// wrong a moment later.
const DEFAULT_PORT: u16 = 32121;

/// A UDP port nobody is using. Taken from a range the operating system never
/// hands out as an ephemeral port (another test's sockets would otherwise
/// take it between the probe and the host's bind), at an offset that differs
/// per process and per call; as `mouser_core::testing::free_port` does. A
/// port is also claimed with a lock file held until the process exits, so test
/// processes running at once (the offsets can coincide) never hand out the
/// same one in the gap before the host binds it.
pub(crate) fn free_port() -> u16 {
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    std::iter::repeat_with(|| {
        let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        20_000 + (std::process::id().wrapping_mul(97).wrapping_add(n) % 20_000) as u16
    })
    .find(|port| {
        *port != DEFAULT_PORT
            && std::net::UdpSocket::bind(("0.0.0.0", *port)).is_ok()
            && std::net::UdpSocket::bind(("::", *port)).is_ok()
            && claim(*port)
    })
    .expect("an endless iterator")
}

/// Lock `port` against every other test process; the lock lasts until exit.
fn claim(port: u16) -> bool {
    static HELD: std::sync::Mutex<Vec<std::fs::File>> = std::sync::Mutex::new(Vec::new());
    let path = std::env::temp_dir().join(format!("mouser-test-port-{port}.lock"));
    let file = std::fs::File::create(path).expect("a lock file can be created");
    let locked = file.try_lock().is_ok();
    HELD.lock().unwrap().push(file);
    locked
}

#[cfg(unix)]
pub(crate) fn control_path_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("mouser.sock")
}

#[cfg(windows)]
pub(crate) fn control_path_in(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let unique = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    std::path::PathBuf::from(format!(r"\\.\pipe\mouser-app-test-{unique}"))
}

/// Where an engine in `mode` that was given `control` serves the control API.
#[cfg(unix)]
pub(crate) fn served_at(
    control: &std::path::Path,
    _mode: mouser_proto::wire::ComputerMode,
) -> std::path::PathBuf {
    control.to_path_buf()
}

/// Where an engine in `mode` that was given `control` serves the control API:
/// the service's engine serves the service pipe.
#[cfg(windows)]
pub(crate) fn served_at(
    control: &std::path::Path,
    mode: mouser_proto::wire::ComputerMode,
) -> std::path::PathBuf {
    match mode {
        mouser_proto::wire::ComputerMode::Service => {
            std::path::PathBuf::from(mouser_core::control::SERVICE_PIPE)
        }
        _ => control.to_path_buf(),
    }
}

/// A host running on a fresh virtual computer named "Desk Mac".
pub async fn fixture() -> Fixture {
    let platform = VirtualPlatform::new();
    fixture_on(platform.clone(), Arc::new(platform)).await
}

/// A host running on `shared`, a platform the test also keeps a handle to.
pub async fn fixture_on(platform: VirtualPlatform, shared: Arc<dyn Platform>) -> Fixture {
    fixture_with(platform, shared, |_| {}).await
}

/// A host whose release location refuses connections, so `update.check`
/// never leaves this computer.
pub async fn fixture_offline() -> Fixture {
    fixture_releasing_at("http://127.0.0.1:1/latest.json").await
}

/// A host that looks for releases at `release_url`, a loopback address.
pub async fn fixture_releasing_at(release_url: &str) -> Fixture {
    let platform = VirtualPlatform::new();
    fixture_with(platform.clone(), Arc::new(platform), |config| {
        config.updates.release_url = release_url.into();
    })
    .await
}

async fn fixture_with(
    platform: VirtualPlatform,
    shared: Arc<dyn Platform>,
    tweak: impl FnOnce(&mut mouser_proto::config::Config),
) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    tweak(&mut config);
    mouser_core::config::save(dir.path(), &config);
    let host = Host::start(
        shared,
        dir.path().to_path_buf(),
        control_path_in(&dir),
        "Desk Mac".into(),
        "1.2.3".into(),
        mouser_proto::wire::ComputerMode::Installed,
        crate::host::Updates::default(),
    )
    .await;
    Fixture {
        host,
        platform,
        _dir: dir,
    }
}

pub async fn status_of(host: &Host) -> Status {
    let value = host.control().call(Method::Status, None).await.unwrap();
    serde_json::from_value(value).unwrap()
}

/// A mock app that manages `host` and has no windows yet. (The mock runtime
/// cannot show or hide windows, so tests observe which windows get created.)
pub fn app_with(host: Host) -> tauri::App<MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(host);
    app
}

pub fn has_window(app: &tauri::AppHandle<MockRuntime>, label: &str) -> bool {
    app.get_webview_window(label).is_some()
}

/// Wait until `condition` holds (work spawned by the app runs in the background).
pub async fn eventually(mut condition: impl FnMut() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        condition(),
        "condition did not become true within 5 seconds"
    );
}

#[derive(Default, Clone)]
pub struct FakeTray {
    pub views: Arc<Mutex<Vec<TrayView>>>,
}

impl TrayPort for FakeTray {
    fn apply(&self, view: &TrayView) {
        self.views.lock().unwrap().push(view.clone());
    }
}

#[derive(Default, Clone)]
pub struct FakeNotifier {
    pub shown: Arc<Mutex<Vec<(String, String)>>>,
    pub opened: Arc<Mutex<Vec<String>>>,
}

impl Notifier for FakeNotifier {
    fn show(&self, title: &str, body: &str) {
        self.shown
            .lock()
            .unwrap()
            .push((title.to_string(), body.to_string()));
    }

    fn open(&self, route: &str) {
        self.opened.lock().unwrap().push(route.to_string());
    }
}

/// A platform that cannot open the system's permission screens.
pub struct NoSettingsScreen(pub VirtualPlatform);

impl Platform for NoSettingsScreen {
    fn info(&self) -> PlatformInfo {
        self.0.info()
    }
    fn displays(&self) -> mouser_platform::Result<Vec<LocalDisplay>> {
        self.0.displays()
    }
    fn watch_displays(
        &self,
        tx: std::sync::mpsc::Sender<Vec<LocalDisplay>>,
    ) -> mouser_platform::Result<WatchHandle> {
        self.0.watch_displays(tx)
    }
    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        self.0.permissions()
    }
    fn open_permission_settings(&self, _kind: PermissionKind) -> mouser_platform::Result<()> {
        Err(mouser_platform::Error::Os(
            "There is no settings screen on this computer.".into(),
        ))
    }
    fn capture(&self) -> mouser_platform::Result<Box<dyn InputCapture>> {
        self.0.capture()
    }
    fn inject(&self) -> mouser_platform::Result<Box<dyn InputInject>> {
        self.0.inject()
    }
    fn clipboard(&self) -> mouser_platform::Result<Box<dyn Clipboard>> {
        self.0.clipboard()
    }
    fn secure_input(&self) -> Option<SecureInputHolder> {
        self.0.secure_input()
    }
    fn foreground_is_fullscreen(&self) -> bool {
        self.0.foreground_is_fullscreen()
    }
    fn interfaces(&self) -> Vec<NetInterface> {
        self.0.interfaces()
    }
    fn watch_network(
        &self,
        tx: std::sync::mpsc::Sender<()>,
    ) -> mouser_platform::Result<WatchHandle> {
        self.0.watch_network(tx)
    }
    fn foreground_app(&self) -> Option<mouser_platform::ForegroundApp> {
        self.0.foreground_app()
    }
    fn secrets(&self) -> Box<dyn SecretStore> {
        self.0.secrets()
    }
    fn autostart(&self) -> Box<dyn Autostart> {
        self.0.autostart()
    }
    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        self.0.file_drag()
    }
}
