//! Helpers for unit tests that run real engines on loopback: an in-process
//! discovery registry and a way to start named nodes and call their API.

use crate::lock::LockExt;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mouser_platform::NetInterface;
use mouser_proto::control::{Method, RpcError};
use mouser_proto::ids::ComputerId;
use mouser_testkit::{SimNet, SimNode, VirtualPlatform};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::discovery::{Advert, Discovery, Found};
use crate::engine::{Engine, EngineParams};
use crate::transport::Transport;

/// In-process stand-in for the network's mDNS. On a simulated network each
/// advert also knows its computer's `SimNode`, so a browser hands out the
/// route through the simulation instead of the real address.
#[derive(Default)]
pub(crate) struct Registry {
    adverts: Mutex<BTreeMap<ComputerId, (Advert, Option<SimNode>)>>,
}

impl Registry {
    pub fn advert_of(&self, computer: ComputerId) -> Option<Advert> {
        self.adverts
            .guard()
            .get(&computer)
            .map(|(advert, _)| advert.clone())
    }

    /// Stop `computer` being visible, as when its mDNS records expire.
    pub fn hide(&self, computer: ComputerId) {
        self.adverts.guard().remove(&computer);
    }
}

pub(crate) struct Browser {
    registry: Arc<Registry>,
    own: Mutex<Option<ComputerId>>,
    me: Option<SimNode>,
    announced: Mutex<Vec<Advert>>,
    interface_changes: Mutex<Vec<Vec<NetInterface>>>,
}

impl Browser {
    /// Every advert announced, in order.
    pub fn announcements(&self) -> Vec<Advert> {
        self.announced.guard().clone()
    }

    /// Every interface list the engine reported after a network change.
    pub fn interface_changes(&self) -> Vec<Vec<NetInterface>> {
        self.interface_changes.guard().clone()
    }
}

impl Discovery for Browser {
    fn announce(&self, advert: &Advert) {
        *self.own.guard() = Some(advert.computer);
        self.announced.guard().push(advert.clone());
        self.registry
            .adverts
            .guard()
            .insert(advert.computer, (advert.clone(), self.me.clone()));
    }

    fn interfaces_changed(&self, interfaces: Vec<NetInterface>) {
        self.interface_changes.guard().push(interfaces);
    }

    fn found(&self) -> Vec<Found> {
        let own = *self.own.guard();
        let adverts = self.registry.adverts.guard();
        adverts
            .values()
            .filter(|(a, _)| Some(a.computer) != own)
            .map(|(a, node)| Found {
                computer: a.computer,
                display_name: a.display_name.clone(),
                protocol: a.protocol,
                desk: a.desk.clone(),
                addrs: vec![match (&self.me, node) {
                    (Some(me), Some(node)) => me.route_to(node),
                    _ => SocketAddr::from(([127, 0, 0, 1], a.port)),
                }],
            })
            .collect()
    }

    fn stop(&self) {
        let own = self.own.guard().take();
        self.registry
            .adverts
            .guard()
            .retain(|id, _| Some(*id) != own);
    }
}

pub(crate) struct Lab {
    pub registry: Arc<Registry>,
    /// The simulated network the computers are on, when they are not on
    /// plain loopback.
    pub net: Option<SimNet>,
}

pub(crate) struct TestNode {
    pub engine: Engine,
    pub platform: VirtualPlatform,
    pub dir: tempfile::TempDir,
    pub control_path: std::path::PathBuf,
    pub discovery: Arc<Browser>,
    /// This computer on the simulated network.
    pub sim: Option<SimNode>,
}

/// The port a default-configured engine listens on. Tests that start an
/// engine without choosing a port take it, so a probe that finds it free is
/// wrong a moment later.
const DEFAULT_PORT: u16 = 32121;

/// A UDP port nobody is using. Taken from a range the operating system never
/// hands out as an ephemeral port (the relay sockets of other computers
/// would otherwise take it between the probe and the engine's bind), at an
/// offset that differs per process and per call.
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
    })
    .expect("an endless iterator")
}

impl Lab {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Registry::default()),
            net: None,
        }
    }

    /// Computers connected through a `SimNet`: every packet between them
    /// can be delayed, dropped, or re-addressed.
    pub fn simulated() -> Self {
        Self {
            net: Some(SimNet::new()),
            ..Self::new()
        }
    }

    pub fn browser(&self) -> Arc<dyn Discovery> {
        self.browser_for(None)
    }

    pub fn browser_for(&self, me: Option<SimNode>) -> Arc<Browser> {
        Arc::new(Browser {
            registry: self.registry.clone(),
            own: Mutex::new(None),
            me,
            announced: Mutex::new(Vec::new()),
            interface_changes: Mutex::new(Vec::new()),
        })
    }

    /// Put a computer on the simulated network (if any), at the port its
    /// config says it will listen on.
    fn sim_node(
        &self,
        name: &str,
        platform: &VirtualPlatform,
        dir: &tempfile::TempDir,
    ) -> Option<SimNode> {
        let net = self.net.as_ref()?;
        let port = crate::config::load_or_create(dir.path()).advanced.port;
        let sim = net.add_node(name, SocketAddr::from(([127, 0, 0, 1], port as u16)));
        sim.bind_platform(platform);
        Some(sim)
    }

    /// Start a node with default settings.
    pub async fn start(&self, name: &str) -> TestNode {
        self.start_with(name, VirtualPlatform::new(), |_| {}).await
    }

    /// Start a node that speaks another protocol version.
    pub async fn start_with_protocol(&self, name: &str, protocol: u16) -> TestNode {
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(free_port());
        crate::config::save(dir.path(), &config);
        self.launch(name, VirtualPlatform::new(), dir, protocol, "9.9.9")
            .await
    }

    async fn launch(
        &self,
        name: &str,
        platform: VirtualPlatform,
        dir: tempfile::TempDir,
        protocol: u16,
        version: &str,
    ) -> TestNode {
        self.launch_with(name, platform, dir, protocol, version, |_| {})
            .await
    }

    async fn launch_with(
        &self,
        name: &str,
        platform: VirtualPlatform,
        dir: tempfile::TempDir,
        protocol: u16,
        version: &str,
        tweak: impl FnOnce(&mut EngineParams),
    ) -> TestNode {
        let control_path = control_path(&dir);
        let sim = self.sim_node(name, &platform, &dir);
        let discovery = self.browser_for(sim.clone());
        let mut params = EngineParams::new(
            Arc::new(platform.clone()),
            dir.path().to_path_buf(),
            control_path.clone(),
            name.to_string(),
            version.to_string(),
        );
        params.protocol_version = protocol;
        params.discovery = Some(discovery.clone());
        tweak(&mut params);
        TestNode {
            engine: Engine::start(params).await,
            platform,
            dir,
            control_path,
            discovery,
            sim,
        }
    }

    /// Start a node on `platform`, letting `tweak` edit the config first.
    pub async fn start_with(
        &self,
        name: &str,
        platform: VirtualPlatform,
        tweak: impl FnOnce(&mut mouser_proto::config::Config),
    ) -> TestNode {
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(free_port());
        tweak(&mut config);
        crate::config::save(dir.path(), &config);
        self.boot(name, platform, dir).await
    }

    /// Start a node, letting `tweak_config` edit its config and
    /// `tweak_params` its engine parameters (version, installer, release
    /// server...).
    pub async fn start_released(
        &self,
        name: &str,
        tweak_config: impl FnOnce(&mut mouser_proto::config::Config),
        tweak_params: impl FnOnce(&mut EngineParams),
    ) -> TestNode {
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(free_port());
        tweak_config(&mut config);
        crate::config::save(dir.path(), &config);
        self.launch_with(
            name,
            VirtualPlatform::new(),
            dir,
            mouser_proto::wire::PROTOCOL_VERSION,
            "0.1.0",
            tweak_params,
        )
        .await
    }

    /// Start an engine over `dir` with `tweak` applied to its parameters.
    pub async fn boot_with(
        &self,
        name: &str,
        platform: VirtualPlatform,
        dir: tempfile::TempDir,
        tweak: impl FnOnce(&mut EngineParams),
    ) -> TestNode {
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.launch_with(
            name,
            platform,
            dir,
            mouser_proto::wire::PROTOCOL_VERSION,
            "0.1.0",
            tweak,
        )
        .await
    }

    /// (Re)start an engine over an existing config directory.
    pub async fn boot(
        &self,
        name: &str,
        platform: VirtualPlatform,
        dir: tempfile::TempDir,
    ) -> TestNode {
        self.boot_with_protocol(name, platform, dir, mouser_proto::wire::PROTOCOL_VERSION)
            .await
    }

    /// [`boot`](Self::boot) speaking another protocol version.
    pub async fn boot_with_protocol(
        &self,
        name: &str,
        platform: VirtualPlatform,
        dir: tempfile::TempDir,
        protocol: u16,
    ) -> TestNode {
        // The previous endpoint's socket is released by its driver task a tick
        // after shutdown; a real restart is a new process and never races.
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.launch(name, platform, dir, protocol, "0.1.0").await
    }
}

pub(crate) async fn call_raw(
    node: &TestNode,
    method: Method,
    params: Value,
) -> Result<Value, RpcError> {
    node.engine.control().call(method, Some(params)).await
}

pub(crate) async fn call<T: DeserializeOwned>(node: &TestNode, method: Method, params: Value) -> T {
    let value = call_raw(node, method, params)
        .await
        .expect("the call succeeds");
    serde_json::from_value(value).unwrap()
}

pub(crate) async fn call_err(node: &TestNode, method: Method, params: Value) -> RpcError {
    call_raw(node, method, params).await.unwrap_err()
}

pub(crate) async fn status(node: &TestNode) -> mouser_proto::control::Status {
    call(node, Method::Status, json!({})).await
}

/// Poll until `done` holds.
pub(crate) async fn eventually<F, Fut>(what: &str, mut done: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let end = Instant::now() + Duration::from_secs(15);
    while !done().await {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// The next notification on `topic`, within 10 s.
pub(crate) async fn recv_topic(
    rx: &mut tokio::sync::broadcast::Receiver<mouser_proto::control::Notification>,
    topic: mouser_proto::control::Topic,
) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let note = rx.recv().await.unwrap();
            if note.method == topic {
                return note.params;
            }
        }
    })
    .await
    .expect("the notification arrives in time")
}

impl Lab {
    /// Start a node with diagnostic logging on.
    pub async fn start_logged(&self, name: &str) -> TestNode {
        self.start_with(name, VirtualPlatform::new(), |c| {
            c.diagnostics.logging = true
        })
        .await
    }
}

impl Lab {
    /// Start a node that runs Mouser `version`, with logging on or off.
    pub async fn start_version(&self, name: &str, version: &str, logging: bool) -> TestNode {
        let version = version.to_string();
        self.start_released(
            name,
            move |c| c.diagnostics.logging = logging,
            move |params| params.mouser_version = version,
        )
        .await
    }
}

/// A desk of nodes with logging on, like [`desk_of`].
pub(crate) async fn logged_desk_of(lab: &Lab, names: &[&str]) -> Vec<TestNode> {
    let mut nodes = Vec::new();
    for name in names {
        nodes.push(lab.start_logged(name).await);
    }
    desk_from(nodes).await
}

/// Every record in the log files at `path` (the older file first). A line
/// that is only partly written yet is not a record.
pub(crate) fn read_log(path: &std::path::Path) -> Vec<Value> {
    [path.with_extension("log.1"), path.to_path_buf()]
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every record in the node's log files.
pub(crate) fn log_records(node: &TestNode) -> Vec<Value> {
    read_log(&node.engine.log().path())
}

/// The records of `event` in the node's log.
pub(crate) fn log_events(node: &TestNode, event: &str) -> Vec<Value> {
    log_records(node)
        .into_iter()
        .filter(|r| r["event"] == event)
        .collect()
}

/// Wait for a record of `event` satisfying `pred`.
pub(crate) async fn logged(node: &TestNode, event: &str, pred: impl Fn(&Value) -> bool) -> Value {
    let found = Mutex::new(None);
    eventually(&format!("{event} to be logged"), || async {
        let hit = log_events(node, event).into_iter().find(&pred);
        let done = hit.is_some();
        *found.guard() = hit;
        done
    })
    .await;
    found.into_inner().unwrap().expect("polled at least once")
}

/// Number of other computers `node` is connected to.
pub(crate) async fn connected(node: &TestNode) -> usize {
    use mouser_proto::control::ConnectionState;
    status(node)
        .await
        .computers
        .iter()
        .filter(|c| c.connection == ConnectionState::Connected)
        .count()
}

/// `joiner` joins the desk of `member` (dialled by address, so that this
/// member gets the prompt) with a one-click approval; returns once both see
/// each other connected.
pub(crate) async fn admit(member: &TestNode, joiner: &TestNode) {
    use mouser_proto::control::{Empty, JoinPending, PendingJoin, Topic};
    let mut notes = member.engine.control().notifications();
    let address = match (&joiner.sim, &member.sim) {
        (Some(from), Some(to)) => from.route_to(to).to_string(),
        _ => format!("127.0.0.1:{}", member.engine.local_addr().port()),
    };
    let _: JoinPending = call(joiner, Method::DeskJoin, json!({ "address": address })).await;
    let item: PendingJoin =
        serde_json::from_value(recv_topic(&mut notes, Topic::JoinRequest).await).unwrap();
    let _: Empty = call(
        member,
        Method::JoinDecide,
        json!({"request_id": item.request_id, "approve": true}),
    )
    .await;
    eventually("both computers to see each other", || async {
        connected(member).await >= 1 && connected(joiner).await >= 1
    })
    .await;
}

/// A hand-driven peer for protocol-level tests.
pub(crate) struct Rogue {
    pub identity: crate::identity::Identity,
    pub transport: crate::transport::Transport,
}

impl Rogue {
    pub fn new(seed: u8) -> Self {
        let identity = crate::identity::Identity::from_seed([seed; 32]);
        let transport = crate::transport::Transport::bind(&identity, 0);
        Self {
            identity,
            transport,
        }
    }

    /// Dial `node` and exchange `Hello`s, claiming to be on `desk`.
    pub async fn dial(
        &self,
        node: &TestNode,
        desk: Option<mouser_proto::ids::DeskId>,
    ) -> (crate::transport::Link, crate::session::handshake::PeerHello) {
        let addr = SocketAddr::from(([127, 0, 0, 1], node.engine.local_addr().port()));
        let conn = self
            .transport
            .connect(addr, Some(node.engine.computer_id()))
            .await
            .unwrap();
        let mut link = crate::transport::Link::open(conn).await.unwrap();
        let hello = mouser_proto::wire::ControlMessage::Hello {
            protocol_version: mouser_proto::wire::PROTOCOL_VERSION,
            mouser_version: "0.0.1".into(),
            computer_id: self.identity.computer_id(),
            display_name: "Rogue".into(),
            os: mouser_proto::model::Os::Linux,
            desk_id: desk,
        };
        let theirs = crate::session::handshake::exchange(&mut link, hello)
            .await
            .unwrap();
        (link, theirs)
    }
}

/// A desk member that is hand-driven, so the way the node closes a link can
/// be read from the other end of it.
pub(crate) async fn member_rogue(
    node: &TestNode,
    desk: &mouser_proto::control::Desk,
    seed: u8,
) -> crate::transport::Link {
    let key: [u8; 32] = mouser_platform::Platform::secrets(&node.platform)
        .get(&crate::state::desk_key_name(&desk.desk_id))
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let rogue = Rogue::new(seed);
    let (mut link, _) = rogue.dial(node, Some(desk.desk_id)).await;
    assert!(matches!(
        link.reader
            .recv::<mouser_proto::wire::ControlMessage>()
            .await,
        Some(mouser_proto::wire::ControlMessage::DeskAuth { .. })
    ));
    let good = crate::session::auth::proof(
        &key,
        &crate::session::auth::export(&link.conn),
        &desk.desk_id,
        &rogue.identity.computer_id(),
    );
    link.writer
        .send(&mouser_proto::wire::ControlMessage::DeskAuth { hmac: good })
        .await
        .unwrap();
    for _ in 0..2 {
        link.reader
            .recv::<mouser_proto::wire::ControlMessage>()
            .await
            .unwrap();
    }
    // The node knows the rogue's (non-existent) display; give it one.
    link
}

pub(crate) fn empty_state() -> mouser_proto::model::DeskState {
    mouser_proto::model::DeskState {
        epoch: 1,
        author: ComputerId([1; 16]),
        members: Vec::new(),
        layout: mouser_proto::model::Layout::default(),
        allowlist: Vec::new(),
    }
}

/// Where a node's control endpoint lives: a pipe named after its directory.
#[cfg(windows)]
fn control_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let unique = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    std::path::PathBuf::from(format!(r"\\.\pipe\mouser-test-{unique}"))
}

/// Where a node's control endpoint lives: a socket in its directory.
#[cfg(not(windows))]
fn control_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
    dir.path().join("mouser.sock")
}

/// A 1920x1080 display of 527x296 mm called `key` at `origin`.
pub(crate) fn screen(key: &str, origin: (f64, f64)) -> mouser_platform::LocalDisplay {
    mouser_platform::LocalDisplay {
        key: key.into(),
        name: key.into(),
        pixel_size: (1920, 1080),
        logical_size: (1920.0, 1080.0),
        scale: 1.0,
        physical_size_mm: Some((527.0, 296.0)),
        origin,
    }
}

/// A desk of the nodes called `names`, the first creating it and admitting
/// the others, once every node knows every display and is connected to all.
pub(crate) async fn desk_of(lab: &Lab, names: &[&str]) -> Vec<TestNode> {
    let mut nodes = Vec::new();
    for name in names {
        nodes.push(lab.start(name).await);
    }
    desk_from(nodes).await
}

/// [`desk_of`] for nodes that are already started.
pub(crate) async fn desk_from(nodes: Vec<TestNode>) -> Vec<TestNode> {
    let _: mouser_proto::control::Desk = call(
        &nodes[0],
        Method::DeskCreate,
        json!({"name": "Home office"}),
    )
    .await;
    for joiner in 1..nodes.len() {
        admit(&nodes[0], &nodes[joiner]).await;
    }
    ready(&nodes).await;
    nodes
}

/// Wait until every node is connected to all others with the whole layout
/// known to its input path.
pub(crate) async fn ready(nodes: &[TestNode]) {
    for node in nodes {
        eventually("every computer to be connected and placed", || async {
            connected(node).await == nodes.len() - 1
                && node.engine.ctx.crossing.guard().map.placed.len() == nodes.len()
        })
        .await;
    }
}

/// Cross the right edge of the node's display with the shared mouse.
pub(crate) fn push_right(node: &TestNode) {
    node.platform
        .send_input(mouser_platform::LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1919.0, 540.0),
        });
}

/// Cross the left edge of the node's display with the shared mouse.
pub(crate) fn push_left(node: &TestNode) {
    node.platform
        .send_input(mouser_platform::LocalInput::Motion {
            dx: -1.0,
            dy: 0.0,
            abs: (0.0, 540.0),
        });
}

/// Wait until the platform has been asked to inject something matching `pred`.
pub(crate) async fn injected_where(
    platform: &VirtualPlatform,
    what: &str,
    pred: impl Fn(&mouser_testkit::Injected) -> bool,
) {
    eventually(what, || async { platform.injected().iter().any(&pred) }).await;
}

/// Send `msg` from `from` to its desk-mate `to` over the control stream.
pub(crate) fn send_to(from: &TestNode, to: &TestNode, msg: mouser_proto::wire::ControlMessage) {
    from.engine.ctx.core.guard().peers[&to.engine.computer_id()].send(msg);
}

/// Send a motion datagram from `from` to `to`.
pub(crate) fn datagram_to(
    from: &TestNode,
    to: &TestNode,
    motion: &mouser_proto::wire::MotionDatagram,
) {
    let bytes = postcard::to_stdvec(motion).unwrap();
    from.engine.ctx.core.guard().peers[&to.engine.computer_id()]
        .conn
        .send_datagram(bytes.into())
        .unwrap();
}

/// Let in-flight messages arrive: a round trip over the control stream.
pub(crate) async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// An interface called `name` of `kind` holding `addr`.
pub(crate) fn interface(
    name: &str,
    kind: mouser_platform::InterfaceKind,
    addr: [u8; 4],
) -> NetInterface {
    NetInterface {
        name: name.into(),
        kind,
        addrs: vec![std::net::IpAddr::from(addr)],
    }
}

/// How `node` holds its connection to `other`: its link id, whether `node`
/// dialled it, and its phase.
pub(crate) fn link_of(
    node: &TestNode,
    other: &TestNode,
) -> Option<(u64, bool, crate::engine::Phase)> {
    node.engine
        .ctx
        .core
        .guard()
        .peers
        .get(&other.engine.computer_id())
        .map(|p| (p.link_id, p.dialed, p.phase))
}

/// `(dialler, acceptor)` indexes of the two-computer desk `nodes`: the
/// computer that dialled the connection and the one that accepted it.
pub(crate) fn roles(nodes: &[TestNode]) -> (usize, usize) {
    let first_dialled = link_of(&nodes[0], &nodes[1]).expect("connected").1;
    let dialler = usize::from(!first_dialled);
    (dialler, 1 - dialler)
}

/// Push the shared mouse of `from` toward `to` (Alpha is left of Beta).
pub(crate) fn push_toward(nodes: &[TestNode], from: usize) {
    if from == 0 {
        push_right(&nodes[0]);
    } else {
        push_left(&nodes[1]);
    }
}

/// Wait until `node` shows `other` as `state`.
pub(crate) async fn wait_connection(
    node: &TestNode,
    other: &TestNode,
    state: mouser_proto::control::ConnectionState,
) -> mouser_proto::control::StatusComputer {
    let id = other.engine.computer_id();
    let seen = Mutex::new(None);
    eventually(&format!("{state:?}"), || async {
        let computer = status(node)
            .await
            .computers
            .into_iter()
            .find(|c| c.id == id)
            .expect("a desk member");
        let done = computer.connection == state;
        *seen.guard() = Some(computer);
        done
    })
    .await;
    seen.into_inner().unwrap().expect("polled at least once")
}

/// Move the shared mouse of `controller`, which has the pointer on the
/// other computer, back across the edge it came through.
pub(crate) fn push_back(nodes: &[TestNode], controller: usize) {
    if controller == 0 {
        push_left(&nodes[0]);
    } else {
        push_right(&nodes[1]);
    }
}

/// A desk of Alpha (one display) and Beta (displays `b1` and `b2`, side by
/// side), with both computers connected and knowing all three placed.
pub(crate) async fn docked_desk(lab: &Lab) -> Vec<TestNode> {
    let beta = VirtualPlatform::new();
    beta.set_displays(vec![screen("b1", (0.0, 0.0)), screen("b2", (1920.0, 0.0))]);
    let nodes = vec![
        lab.start("Alpha").await,
        lab.start_with("Beta", beta, |_| {}).await,
    ];
    let _: mouser_proto::control::Desk = call(
        &nodes[0],
        Method::DeskCreate,
        json!({"name": "Home office"}),
    )
    .await;
    admit(&nodes[0], &nodes[1]).await;
    for node in &nodes {
        eventually("all three displays placed", || async {
            mapped(node).len() == 3
        })
        .await;
    }
    nodes
}

/// The keys of the displays on `node`'s input map, sorted.
pub(crate) fn mapped(node: &TestNode) -> Vec<String> {
    let mut keys: Vec<_> = node
        .engine
        .ctx
        .crossing
        .guard()
        .map
        .placed
        .iter()
        .map(|p| p.display.display_id.key.clone())
        .collect();
    keys.sort();
    keys
}

/// `layout.get` of `node`.
pub(crate) async fn layout_of(node: &TestNode) -> Value {
    call(node, Method::LayoutGet, json!({})).await
}

/// The desk epoch `node` holds.
pub(crate) fn epoch_of(node: &TestNode) -> u64 {
    node.engine
        .ctx
        .core
        .guard()
        .state
        .desk
        .as_ref()
        .expect("on a desk")
        .state
        .epoch
}

/// How long a handshake between two endpoints on this machine may take
/// before a test gives up. The transport's own 3 s idle timeout ends a
/// handshake that stalls, so this is only the backstop for a side that is
/// never told.
const HANDSHAKE: Duration = Duration::from_secs(30);

/// `dialer` connects to `to` and `acceptor` accepts: the dialled and the
/// accepted connection. Never waits forever: if either side fails or the
/// handshake stalls, the test fails saying so (a side that never sees the
/// attempt would otherwise be waited for indefinitely).
pub(crate) async fn handshake(
    dialer: &Transport,
    acceptor: &Transport,
    to: SocketAddr,
    expected: Option<ComputerId>,
) -> (quinn::Connection, quinn::Connection) {
    let dial = async {
        dialer
            .connect(to, expected)
            .await
            .expect("the dialling side connects")
    };
    let accept = async {
        let incoming = acceptor.accept().await.expect("the endpoint is open");
        Transport::complete(incoming)
            .await
            .expect("the accepting side completes the handshake")
    };
    tokio::time::timeout(HANDSHAKE, async { tokio::join!(dial, accept) })
        .await
        .expect("the handshake finishes in time")
}

/// A computer whose input cannot be grabbed (as on Wayland): everything
/// else is the wrapped virtual computer.
pub(crate) struct NoGrab(pub VirtualPlatform);

struct NoGrabCapture(Box<dyn mouser_platform::InputCapture>);

impl mouser_platform::InputCapture for NoGrabCapture {
    fn start(
        &mut self,
        tx: std::sync::mpsc::Sender<mouser_platform::LocalInput>,
    ) -> mouser_platform::Result<()> {
        self.0.start(tx)
    }

    fn set_grabbed(&mut self, _grabbed: bool) -> mouser_platform::Result<()> {
        Err(mouser_platform::Error::Unsupported)
    }

    fn warp(&mut self, x: f64, y: f64) -> mouser_platform::Result<()> {
        self.0.warp(x, y)
    }

    fn stop(&mut self) {
        self.0.stop()
    }
}

/// Knobs of a [`Flaky`] computer, shared with the test.
#[derive(Clone, Default)]
pub(crate) struct Knobs {
    /// `set_grabbed(true)` fails with an operating-system error.
    pub refuse_grab: Arc<AtomicBool>,
    /// `warp` fails with an operating-system error.
    pub refuse_warp: Arc<AtomicBool>,
    /// The next `secure_input()` (asked on every reconcile tick) panics.
    pub panic_secure_input: Arc<AtomicBool>,
    /// Every `set_grabbed` call that reached the capture, in order.
    pub grabs: Arc<Mutex<Vec<bool>>>,
}

/// A virtual computer whose capture and platform can be made to fail.
pub(crate) struct Flaky {
    pub inner: VirtualPlatform,
    pub knobs: Knobs,
}

pub(crate) struct FlakyCapture {
    inner: Box<dyn mouser_platform::InputCapture>,
    knobs: Knobs,
}

impl mouser_platform::InputCapture for FlakyCapture {
    fn start(
        &mut self,
        tx: std::sync::mpsc::Sender<mouser_platform::LocalInput>,
    ) -> mouser_platform::Result<()> {
        self.inner.start(tx)
    }

    fn set_grabbed(&mut self, grabbed: bool) -> mouser_platform::Result<()> {
        self.knobs.grabs.guard().push(grabbed);
        if grabbed && self.knobs.refuse_grab.load(Ordering::SeqCst) {
            return Err(mouser_platform::Error::Os("grab refused".into()));
        }
        self.inner.set_grabbed(grabbed)
    }

    fn warp(&mut self, x: f64, y: f64) -> mouser_platform::Result<()> {
        if self.knobs.refuse_warp.load(Ordering::SeqCst) {
            return Err(mouser_platform::Error::Os("warp refused".into()));
        }
        self.inner.warp(x, y)
    }

    fn stop(&mut self) {
        self.inner.stop()
    }
}

impl mouser_platform::Platform for Flaky {
    fn info(&self) -> mouser_platform::PlatformInfo {
        self.inner.info()
    }

    fn displays(&self) -> mouser_platform::Result<Vec<mouser_platform::LocalDisplay>> {
        self.inner.displays()
    }

    fn watch_displays(
        &self,
        tx: std::sync::mpsc::Sender<Vec<mouser_platform::LocalDisplay>>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.inner.watch_displays(tx)
    }

    fn permissions(
        &self,
    ) -> Vec<(
        mouser_platform::PermissionKind,
        mouser_platform::PermissionState,
    )> {
        self.inner.permissions()
    }

    fn open_permission_settings(
        &self,
        kind: mouser_platform::PermissionKind,
    ) -> mouser_platform::Result<()> {
        self.inner.open_permission_settings(kind)
    }

    fn capture(&self) -> mouser_platform::Result<Box<dyn mouser_platform::InputCapture>> {
        Ok(Box::new(FlakyCapture {
            inner: self.inner.capture()?,
            knobs: self.knobs.clone(),
        }))
    }

    fn inject(&self) -> mouser_platform::Result<Box<dyn mouser_platform::InputInject>> {
        self.inner.inject()
    }

    fn clipboard(&self) -> mouser_platform::Result<Box<dyn mouser_platform::Clipboard>> {
        self.inner.clipboard()
    }

    fn secure_input(&self) -> Option<mouser_platform::SecureInputHolder> {
        if self.knobs.panic_secure_input.swap(false, Ordering::SeqCst) {
            panic!("scripted platform fault");
        }
        self.inner.secure_input()
    }

    fn foreground_is_fullscreen(&self) -> bool {
        self.inner.foreground_is_fullscreen()
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        self.inner.interfaces()
    }

    fn watch_network(
        &self,
        tx: std::sync::mpsc::Sender<()>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.inner.watch_network(tx)
    }

    fn watch_problems(
        &self,
        tx: std::sync::mpsc::Sender<mouser_platform::PlatformProblem>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.inner.watch_problems(tx)
    }

    fn foreground_app(&self) -> Option<mouser_platform::ForegroundApp> {
        self.inner.foreground_app()
    }

    fn secrets(&self) -> Box<dyn mouser_platform::SecretStore> {
        self.inner.secrets()
    }

    fn autostart(&self) -> Box<dyn mouser_platform::Autostart> {
        self.inner.autostart()
    }

    fn file_drag(&self) -> Option<Box<dyn mouser_platform::FileDrag>> {
        self.inner.file_drag()
    }
}

impl mouser_platform::Platform for NoGrab {
    fn info(&self) -> mouser_platform::PlatformInfo {
        self.0.info()
    }

    fn displays(&self) -> mouser_platform::Result<Vec<mouser_platform::LocalDisplay>> {
        self.0.displays()
    }

    fn watch_displays(
        &self,
        tx: std::sync::mpsc::Sender<Vec<mouser_platform::LocalDisplay>>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.0.watch_displays(tx)
    }

    fn permissions(
        &self,
    ) -> Vec<(
        mouser_platform::PermissionKind,
        mouser_platform::PermissionState,
    )> {
        self.0.permissions()
    }

    fn open_permission_settings(
        &self,
        kind: mouser_platform::PermissionKind,
    ) -> mouser_platform::Result<()> {
        self.0.open_permission_settings(kind)
    }

    fn capture(&self) -> mouser_platform::Result<Box<dyn mouser_platform::InputCapture>> {
        Ok(Box::new(NoGrabCapture(self.0.capture()?)))
    }

    fn inject(&self) -> mouser_platform::Result<Box<dyn mouser_platform::InputInject>> {
        self.0.inject()
    }

    fn clipboard(&self) -> mouser_platform::Result<Box<dyn mouser_platform::Clipboard>> {
        self.0.clipboard()
    }

    fn secure_input(&self) -> Option<mouser_platform::SecureInputHolder> {
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
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.0.watch_network(tx)
    }

    fn watch_problems(
        &self,
        tx: std::sync::mpsc::Sender<mouser_platform::PlatformProblem>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.0.watch_problems(tx)
    }

    fn foreground_app(&self) -> Option<mouser_platform::ForegroundApp> {
        self.0.foreground_app()
    }

    fn secrets(&self) -> Box<dyn mouser_platform::SecretStore> {
        self.0.secrets()
    }

    fn autostart(&self) -> Box<dyn mouser_platform::Autostart> {
        self.0.autostart()
    }

    fn file_drag(&self) -> Option<Box<dyn mouser_platform::FileDrag>> {
        self.0.file_drag()
    }
}

impl Lab {
    /// Start a node whose capture and platform fail when its knobs say so.
    pub async fn start_flaky(&self, name: &str) -> (TestNode, Knobs) {
        let platform = VirtualPlatform::new();
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(free_port());
        crate::config::save(dir.path(), &config);
        let knobs = Knobs::default();
        let flaky = Flaky {
            inner: platform.clone(),
            knobs: knobs.clone(),
        };
        let node = self
            .boot_with(name, platform, dir, move |params| {
                params.platform = Arc::new(flaky);
            })
            .await;
        (node, knobs)
    }

    /// Start a node whose input cannot be grabbed.
    pub async fn start_ungrabbable(&self, name: &str) -> TestNode {
        let platform = VirtualPlatform::new();
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(free_port());
        crate::config::save(dir.path(), &config);
        let wrapped = platform.clone();
        self.boot_with(name, platform, dir, move |params| {
            params.platform = Arc::new(NoGrab(wrapped));
        })
        .await
    }
}

#[test]
fn the_ungrabbable_computer_is_the_wrapped_one_in_every_other_way() {
    use mouser_platform::{PermissionKind, Platform};
    let inner = VirtualPlatform::new();
    inner.set_foreground_app(None);
    let platform = NoGrab(inner.clone());
    assert!(platform.foreground_app().is_none());
    platform
        .open_permission_settings(PermissionKind::Accessibility)
        .unwrap();
    assert_eq!(
        inner.opened_permission_settings(),
        [PermissionKind::Accessibility]
    );
    let mut capture = platform.capture().unwrap();
    capture.start(std::sync::mpsc::channel().0).unwrap();
    assert!(inner.is_capturing());
    capture.stop();
    assert!(!inner.is_capturing());
}

#[test]
fn the_flaky_computer_is_the_wrapped_one_in_every_other_way() {
    use mouser_platform::{PermissionKind, Platform};
    let inner = VirtualPlatform::new();
    inner.set_foreground_app(None);
    let knobs = Knobs::default();
    let platform = Flaky {
        inner: inner.clone(),
        knobs: knobs.clone(),
    };
    assert!(platform.foreground_app().is_none());
    platform
        .open_permission_settings(PermissionKind::Accessibility)
        .unwrap();
    assert_eq!(
        inner.opened_permission_settings(),
        [PermissionKind::Accessibility]
    );
    let mut capture = platform.capture().unwrap();
    capture.start(std::sync::mpsc::channel().0).unwrap();
    assert!(inner.is_capturing());
    capture.set_grabbed(true).unwrap();
    assert!(inner.is_grabbed());
    knobs.refuse_grab.store(true, Ordering::SeqCst);
    assert!(capture.set_grabbed(true).is_err());
    knobs.refuse_warp.store(true, Ordering::SeqCst);
    assert!(capture.warp(1.0, 1.0).is_err());
    knobs.refuse_warp.store(false, Ordering::SeqCst);
    capture.warp(1.0, 1.0).unwrap();
    capture.stop();
    assert!(!inner.is_capturing());
    assert_eq!(*knobs.grabs.guard(), [true, true]);
}
