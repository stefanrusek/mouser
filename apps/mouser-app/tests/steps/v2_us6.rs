//! Step definitions for `v2_us6.feature`: headless Mouser on Windows and
//! Linux. The headless host is started through the same functions the app's
//! `--headless` launch uses; the desk's other computer is an ordinary
//! in-process engine. Discovery is an in-process registry, so nothing is
//! announced on the real network.

use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_app::{headless, mode};
use mouser_core::discovery::{Advert, Discovery, Found};
use mouser_core::paths::Paths;
use mouser_core::{ControlClient, Engine, EngineParams};
use mouser_platform::{Capability, Limitation, LocalInput, Os, PlatformInfo, SessionType};
use mouser_proto::control::{ConnectionState, Method, Status, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ComputerMode;
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(20);

const LOGIN_SCREEN: &str = "Mouser can't control this computer's login screen.";

/// In-process stand-in for the network's mDNS, shared by both computers.
#[derive(Default)]
struct Registry {
    adverts: Mutex<BTreeMap<ComputerId, Advert>>,
}

struct Sim {
    registry: Arc<Registry>,
    own: Mutex<Option<ComputerId>>,
}

impl Discovery for Sim {
    fn announce(&self, advert: &Advert) {
        *self.own.lock().unwrap() = Some(advert.computer);
        self.registry
            .adverts
            .lock()
            .unwrap()
            .insert(advert.computer, advert.clone());
    }

    fn found(&self) -> Vec<Found> {
        let own = *self.own.lock().unwrap();
        self.registry
            .adverts
            .lock()
            .unwrap()
            .values()
            .filter(|ad| Some(ad.computer) != own)
            .map(|ad| Found {
                computer: ad.computer,
                display_name: ad.display_name.clone(),
                protocol: ad.protocol,
                desk: ad.desk.clone(),
                addrs: vec![SocketAddr::from(([127, 0, 0, 1], ad.port))],
            })
            .collect()
    }

    fn stop(&self) {
        if let Some(own) = self.own.lock().unwrap().take() {
            self.registry.adverts.lock().unwrap().remove(&own);
        }
    }
}

/// A UDP port nobody is using, from a range the OS never hands out as an
/// ephemeral port.
fn free_port() -> u16 {
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    std::iter::repeat_with(|| {
        let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        20_000 + (std::process::id().wrapping_mul(97).wrapping_add(n) % 20_000) as u16
    })
    .find(|port| *port != 32121 && port_is_free(*port))
    .expect("an endless iterator")
}

/// Whether nothing holds `port` on either address family.
fn port_is_free(port: u16) -> bool {
    std::net::UdpSocket::bind(("0.0.0.0", port)).is_ok()
        && std::net::UdpSocket::bind(("::", port)).is_ok()
}

fn control_path(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!(
            r"\\.\pipe\mouser-acc-us6-{}-{name}-{}",
            std::process::id(),
            free_port()
        ))
    } else {
        dir.path().join("mouser.sock")
    }
}

fn linux_platform(limitations: Vec<Limitation>) -> VirtualPlatform {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os: Os::Linux,
        version: "test".into(),
        session: SessionType::X11,
        limitations,
    });
    platform
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// A computer the test drives: its folder, platform and control socket.
struct Computer {
    name: &'static str,
    dir: tempfile::TempDir,
    platform: VirtualPlatform,
    control: PathBuf,
    client: Option<ControlClient>,
}

impl Computer {
    fn new(name: &'static str, platform: VirtualPlatform) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(free_port());
        mouser_core::config::save(dir.path(), &config);
        let control = control_path(&dir, name);
        Self {
            name,
            dir,
            platform,
            control,
            client: None,
        }
    }

    fn params(&self, registry: &Arc<Registry>, mode: ComputerMode) -> EngineParams {
        self.params_in(registry, mode, self.dir.path().to_path_buf())
    }

    fn params_in(
        &self,
        registry: &Arc<Registry>,
        mode: ComputerMode,
        data_dir: PathBuf,
    ) -> EngineParams {
        let mut params = headless::params(
            Arc::new(self.platform.clone()),
            Os::Linux,
            data_dir,
            self.control.clone(),
            self.name.into(),
            "0.3.0".into(),
            mode,
        );
        params.discovery = Some(Arc::new(Sim {
            registry: registry.clone(),
            own: Mutex::new(None),
        }));
        params
    }

    async fn connect(&mut self) {
        self.client = Some(
            ControlClient::connect(&self.control)
                .await
                .expect("control socket"),
        );
    }

    async fn call(&mut self, method: Method, params: Value) -> Value {
        self.client
            .as_mut()
            .expect("connected")
            .call(method, Some(params))
            .await
            .unwrap_or_else(|e| panic!("{} {method:?}: {}", self.name, e.message))
    }

    async fn status(&mut self) -> Status {
        let value = self.call(Method::Status, json!({})).await;
        serde_json::from_value(value).expect("a status")
    }

    async fn wait_status(&mut self, what: &str, done: impl Fn(&Status) -> bool) -> Status {
        let end = Instant::now() + WAIT;
        loop {
            let status = self.status().await;
            if done(&status) {
                return status;
            }
            assert!(
                Instant::now() < end,
                "{}: timed out waiting for {what}\n{status:#?}",
                self.name
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// The headless host, running until told to stop.
struct Running {
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

#[derive(World)]
#[world(init = Self::new)]
pub struct HeadlessWorld {
    registry: Arc<Registry>,
    alpha: Option<(Computer, Engine)>,
    beta: Option<Computer>,
    running: Option<Running>,
}

impl fmt::Debug for HeadlessWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HeadlessWorld")
    }
}

impl HeadlessWorld {
    fn new() -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            alpha: None,
            beta: None,
            running: None,
        }
    }

    fn alpha(&mut self) -> &mut Computer {
        &mut self.alpha.as_mut().expect("Alpha is running").0
    }

    fn beta(&mut self) -> &mut Computer {
        self.beta.as_mut().expect("Beta exists")
    }

    /// Start Beta the way a headless launch does: `--headless` on `os`.
    async fn start_headless(&mut self, os: Os) {
        let launch = ["mouser", "--headless"].map(String::from);
        let mode = mode::select(&launch, false, false);
        let registry = self.registry.clone();
        let beta = self.beta();
        let folder = Paths {
            dir: beta.dir.path().to_path_buf(),
            portable: false,
        };
        let params = beta.params_in(&registry, mode, headless::data_dir(os, mode, &folder));
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn(headless::run(params, async {
            stopped.await.unwrap();
        }));
        let control = self.beta().control.clone();
        until("the control socket or the host ending", || {
            control.exists() || task.is_finished()
        })
        .await;
        if task.is_finished() {
            // The host ended before it served: say why, not that it was late.
            match task.await {
                Err(fault) => std::panic::resume_unwind(fault.into_panic()),
                Ok(()) => panic!("the headless host ended before opening its control socket"),
            }
        }
        self.running = Some(Running { stop, task });
        self.beta().connect().await;
    }

    pub async fn stop(&mut self) {
        self.beta().client = None;
        if let Some(Running { stop, task }) = self.running.take() {
            // A host that already ended says why (its panic) before anything
            // is made of the stop signal going nowhere.
            let asked = stop.send(());
            if let Err(fault) = task.await {
                std::panic::resume_unwind(fault.into_panic());
            }
            asked.expect("the headless host was still running to be stopped");
        }
        if let Some((_, engine)) = self.alpha.take() {
            engine.shutdown().await;
        }
    }
}

const MOTION_PAST_RIGHT_EDGE: LocalInput = LocalInput::Motion {
    dx: 1.0,
    dy: 0.0,
    abs: (1919.0, 540.0),
};

#[given("a Windows or Linux computer on a desk")]
async fn on_a_desk(world: &mut HeadlessWorld) {
    let registry = world.registry.clone();
    let mut alpha = Computer::new("Alpha", linux_platform(Vec::new()));
    let engine = Engine::start(alpha.params(&registry, ComputerMode::Installed)).await;
    alpha.connect().await;
    world.alpha = Some((alpha, engine));
    world.beta = Some(Computer::new("Beta", linux_platform(Vec::new())));

    // Beta, started as an ordinary engine once, joins Alpha's desk.
    let beta_engine = Engine::start(world.beta().params(&registry, ComputerMode::Installed)).await;
    world.beta().connect().await;
    let mut notes = ControlClient::connect(&world.alpha().control)
        .await
        .unwrap();
    notes.subscribe(vec![Topic::JoinRequest]).await.unwrap();
    world
        .alpha()
        .call(Method::DeskCreate, json!({ "name": "Home office" }))
        .await;
    let address = format!("127.0.0.1:{}", port_of(world.alpha()));
    world
        .beta()
        .call(Method::DeskJoin, json!({ "address": address }))
        .await;
    let request = loop {
        let note = tokio::time::timeout(WAIT, notes.next_notification())
            .await
            .expect("a join request arrives")
            .expect("the control socket stays open");
        if note.method == Topic::JoinRequest && note.params["computer"]["name"] == "Beta" {
            break note.params;
        }
    };
    world
        .alpha()
        .call(
            Method::JoinDecide,
            json!({"request_id": request["request_id"], "approve": true}),
        )
        .await;
    world
        .alpha()
        .wait_status("Beta connected with its display", |s| {
            s.computers.iter().any(|c| {
                c.name == "Beta" && c.connection == ConnectionState::Connected && c.displays > 0
            })
        })
        .await;
    let alpha_engine = &world.alpha.as_ref().unwrap().1;
    until("both displays placed", || {
        alpha_engine.layout().placements.len() == 2
    })
    .await;
    // Beta's user quits Mouser; the desk remembers it.
    world.beta().client = None;
    beta_engine.shutdown().await;
    // The endpoint's socket is released by its driver task a moment after
    // shutdown returns (see mouser-core testing.rs `boot_with`); the headless
    // Beta binds the same port, so wait for the port rather than race it.
    let beta_port = port_of(world.beta()) as u16;
    until("Beta's port to be released", || port_is_free(beta_port)).await;
}

fn port_of(computer: &Computer) -> u32 {
    mouser_core::config::load_or_create(computer.dir.path())
        .advanced
        .port
}

#[when("the desk owner starts Mouser in headless mode")]
async fn starts_headless(world: &mut HeadlessWorld) {
    world.start_headless(Os::Linux).await;
}

#[then(
    "no window or tray icon appears, the computer is on its desk, the desk user can cross to it, and other desk computers list it as connected"
)]
async fn on_its_desk(world: &mut HeadlessWorld) {
    // Nothing but the engine runs: no app, window or tray was built; the
    // computer reports that it runs headless.
    let status = world
        .beta()
        .wait_status("Beta on the desk", |s| s.desk.is_some())
        .await;
    let me = status
        .computers
        .iter()
        .find(|c| c.id == status.computer.id)
        .expect("itself in the list");
    assert_eq!(me.mode, Some(ComputerMode::Headless));
    assert_eq!(status.desk.unwrap().name, "Home office");

    world
        .alpha()
        .wait_status("Beta listed as connected", |s| {
            s.computers.iter().any(|c| {
                c.name == "Beta" && c.connection == ConnectionState::Connected && c.displays > 0
            })
        })
        .await;

    // The desk user's pointer crosses from Alpha onto Beta.
    let alpha = world.alpha().platform.clone();
    let beta = world.beta().platform.clone();
    let landed = || {
        beta.injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. }))
    };
    alpha.send_input(MOTION_PAST_RIGHT_EDGE);
    until("the pointer to reach Beta", landed).await;
}

#[given("Mouser runs on a Linux computer")]
async fn runs_on_linux(world: &mut HeadlessWorld) {
    let platform = linux_platform(vec![Limitation {
        capability: Capability::LoginScreen,
        text: LOGIN_SCREEN.into(),
    }]);
    world.beta = Some(Computer::new("Beta", platform));
    world.start_headless(Os::Linux).await;
}

#[when("the desk owner opens its Health entry")]
async fn opens_health(_world: &mut HeadlessWorld) {
    // The Health entry lists the computer's `status` limitations; the check
    // reads them in the next step.
}

#[then("\"What doesn't work here\" states that Mouser cannot control this computer's login screen")]
async fn login_screen_limitation(world: &mut HeadlessWorld) {
    let status = world.beta().status().await;
    let me = status
        .computers
        .iter()
        .find(|c| c.id == status.computer.id)
        .expect("itself in the list");
    assert_eq!(me.limitations, vec![LOGIN_SCREEN.to_string()]);
}
