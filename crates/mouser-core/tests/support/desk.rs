//! The multi-node desk harness: N engines, each with its own `VirtualPlatform`,
//! temp config directory and `SimNet` node, started in-process.
//!
//! Real mDNS multicast is not routed through `SimNet`, so the engines' discovery
//! is backed by an in-process registry (`SimDiscovery`): it implements the same
//! `Discovery` trait the real `MdnsDiscovery` does, announces and lists the same
//! records, and hands each engine the `SimNet` route to reach a peer instead of
//! its real address. Everything else (QUIC, handshakes, the control API) is the
//! production code path.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mouser_core::discovery::{Advert, Discovery, Found};
use mouser_core::{ControlClient, Engine, EngineEvent, EngineParams};
use mouser_proto::config::Config;
use mouser_proto::control::{Method, Notification, RpcError, Status, Topic};
use mouser_proto::ids::ComputerId;
use mouser_testkit::{SimNet, SimNode, VirtualPlatform};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(20);

/// In-process stand-in for the network's mDNS.
#[derive(Default)]
pub struct Registry {
    adverts: Mutex<BTreeMap<ComputerId, (Advert, SimNode)>>,
}

/// One engine's view of the registry.
struct SimDiscovery {
    registry: Arc<Registry>,
    me: SimNode,
    own: Mutex<Option<ComputerId>>,
    /// A computer that cannot find anyone automatically (Story 1 scenario 6).
    blind: bool,
}

impl Discovery for SimDiscovery {
    fn announce(&self, advert: &Advert) {
        // The advert carries the port the endpoint listens on now. The
        // network delivers a computer's packets to wherever its endpoint is
        // bound, also after the endpoint moved to follow a network change.
        let port = advert.port;
        if self.me.addr().port() != port {
            self.me
                .change_address(SocketAddr::from(([127, 0, 0, 1], port)));
        }
        *self.own.lock().unwrap() = Some(advert.computer);
        self.registry
            .adverts
            .lock()
            .unwrap()
            .insert(advert.computer, (advert.clone(), self.me.clone()));
    }

    fn found(&self) -> Vec<Found> {
        if self.blind {
            return Vec::new();
        }
        let own = *self.own.lock().unwrap();
        let adverts = self.registry.adverts.lock().unwrap();
        adverts
            .values()
            .filter(|(ad, _)| Some(ad.computer) != own)
            .map(|(ad, node)| Found {
                computer: ad.computer,
                display_name: ad.display_name.clone(),
                protocol: ad.protocol,
                desk: ad.desk.clone(),
                addrs: vec![self.me.route_to(node)],
            })
            .collect()
    }

    fn stop(&self) {
        if let Some(own) = self.own.lock().unwrap().take() {
            self.registry.adverts.lock().unwrap().remove(&own);
        }
    }
}

/// Notifications received so far, in order.
#[derive(Default)]
pub struct Events {
    log: Mutex<Vec<Notification>>,
    engine: Mutex<Vec<EngineEvent>>,
}

impl Events {
    fn find(&self, topic: Topic, pred: &dyn Fn(&Value) -> bool) -> Option<Value> {
        let log = self.log.lock().unwrap();
        log.iter()
            .find(|n| n.method == topic && pred(&n.params))
            .map(|n| n.params.clone())
    }

    /// Every engine event (landing, review offers) so far.
    pub fn engine_events(&self) -> Vec<EngineEvent> {
        self.engine.lock().unwrap().clone()
    }

    /// Every notification on `topic` so far.
    pub fn all(&self, topic: Topic) -> Vec<Value> {
        let log = self.log.lock().unwrap();
        log.iter()
            .filter(|n| n.method == topic)
            .map(|n| n.params.clone())
            .collect()
    }
}

/// How a node is set up.
#[derive(Clone)]
pub struct Setup {
    pub platform: VirtualPlatform,
    pub config: Config,
    pub protocol_version: u16,
    pub mouser_version: String,
    pub blind: bool,
    /// The kinds of clipboard content this computer can hold (default: all).
    pub clipboard_holds: Option<fn(&mouser_platform::ClipRep) -> bool>,
    /// The app's installer for update bundles.
    pub installer: Option<mouser_core::release::Installer>,
    /// The updater public key releases must be signed with (default: Mouser's).
    pub release_pubkey: Option<String>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            platform: VirtualPlatform::new(),
            config: Config::default(),
            protocol_version: mouser_proto::wire::PROTOCOL_VERSION,
            mouser_version: "0.1.0".into(),
            blind: false,
            clipboard_holds: None,
            installer: None,
            release_pubkey: None,
        }
    }
}

/// One computer: its virtual OS, its engine, and clients of its control API.
pub struct Node {
    pub name: String,
    pub platform: VirtualPlatform,
    pub sim: SimNode,
    pub dir: tempfile::TempDir,
    pub events: Arc<Events>,
    setup: Setup,
    control_path: PathBuf,
    engine: Option<Engine>,
    client: Option<ControlClient>,
}

static PIPES: AtomicU64 = AtomicU64::new(0);

/// A UDP port nobody is using. Taken from a range the operating system never
/// hands out as an ephemeral port (the relay sockets of other computers
/// would otherwise take it between the probe and the engine's bind), at an
/// offset that differs per process and per call.
fn free_port() -> u16 {
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    std::iter::repeat_with(|| {
        let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        20_000 + (std::process::id().wrapping_mul(97).wrapping_add(n) % 20_000) as u16
    })
    .find(|port| std::net::UdpSocket::bind(("::", *port)).is_ok())
    .expect("an endless iterator")
}

impl Node {
    pub fn id(&self) -> ComputerId {
        self.engine.as_ref().expect("node is running").computer_id()
    }

    /// The version this computer reports the next time it starts.
    pub fn set_version(&mut self, version: &str) {
        self.setup.mouser_version = version.into();
    }

    /// The control-API client of this node.
    fn client(&mut self) -> &mut ControlClient {
        self.client.as_mut().expect("node is running")
    }

    pub async fn call_raw(&mut self, method: Method, params: Value) -> Result<Value, RpcError> {
        self.client().call(method, Some(params)).await
    }

    pub async fn call<T: DeserializeOwned>(&mut self, method: Method, params: Value) -> T {
        let value = self
            .call_raw(method, params)
            .await
            .unwrap_or_else(|e| panic!("{} {method:?}: {}", self.name, e.message));
        serde_json::from_value(value).expect("result matches its type")
    }

    pub async fn status(&mut self) -> Status {
        self.call(Method::Status, json!({})).await
    }

    /// Poll `status` until `pred` holds.
    pub async fn wait_status(&mut self, what: &str, pred: impl Fn(&Status) -> bool) -> Status {
        let end = Instant::now() + WAIT;
        loop {
            let status = self.status().await;
            if pred(&status) {
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

    /// Wait for a notification on `topic` whose params satisfy `pred`.
    pub async fn notification(
        &self,
        what: &str,
        topic: Topic,
        pred: impl Fn(&Value) -> bool,
    ) -> Value {
        let end = Instant::now() + WAIT;
        loop {
            if let Some(found) = self.events.find(topic, &pred) {
                return found;
            }
            assert!(
                Instant::now() < end,
                "{}: timed out waiting for {what}",
                self.name
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// The first engine event matching `pred` from the `skip`th one on.
    pub async fn engine_event(
        &self,
        what: &str,
        skip: usize,
        pred: impl Fn(&EngineEvent) -> bool,
    ) -> EngineEvent {
        let end = Instant::now() + WAIT;
        loop {
            let events = self.events.engine_events();
            if let Some(found) = events.into_iter().skip(skip).find(&pred) {
                return found;
            }
            assert!(
                Instant::now() < end,
                "{}: timed out waiting for {what}",
                self.name
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// The control API in-process, as the app's user interface uses it.
    pub fn control(&self) -> mouser_core::ControlHandle {
        self.engine.as_ref().expect("node is running").control()
    }

    /// The desk layout this computer holds.
    pub fn layout(&self) -> mouser_proto::model::Layout {
        self.engine.as_ref().expect("node is running").layout()
    }

    /// Displays awaiting the owner's review of their placement.
    pub fn needs_review(&self) -> Vec<mouser_proto::model::DisplayId> {
        self.engine
            .as_ref()
            .expect("node is running")
            .needs_review()
    }

    async fn start(&mut self, registry: &Arc<Registry>) {
        let discovery = SimDiscovery {
            registry: registry.clone(),
            me: self.sim.clone(),
            own: Mutex::new(None),
            blind: self.setup.blind,
        };
        let platform: Arc<dyn mouser_platform::Platform> = match self.setup.clipboard_holds {
            None => Arc::new(self.platform.clone()),
            Some(holds) => Arc::new(super::picky::Picky {
                inner: self.platform.clone(),
                holds,
            }),
        };
        let mut params = EngineParams::new(
            platform,
            self.dir.path().to_path_buf(),
            self.control_path.clone(),
            self.name.clone(),
            self.setup.mouser_version.clone(),
        );
        params.protocol_version = self.setup.protocol_version;
        params.discovery = Some(Arc::new(discovery));
        params.installer = self.setup.installer.clone();
        if let Some(key) = &self.setup.release_pubkey {
            params.release_pubkey = key.clone();
        }
        let engine = Engine::start(params).await;
        self.engine = Some(engine);
        self.client = Some(
            ControlClient::connect(&self.control_path)
                .await
                .expect("control socket"),
        );
        let mut stream = ControlClient::connect(&self.control_path)
            .await
            .expect("control socket");
        stream
            .subscribe(vec![
                Topic::Problem,
                Topic::JoinRequest,
                Topic::Status,
                Topic::Transfer,
            ])
            .await
            .expect("subscribe");
        self.events = Arc::new(Events::default());
        let events = self.events.clone();
        let mut engine_events = self.engine.as_ref().expect("started").events();
        let sink = events.clone();
        tokio::spawn(async move {
            while let Ok(event) = engine_events.recv().await {
                sink.engine.lock().unwrap().push(event);
            }
        });
        tokio::spawn(async move {
            while let Some(note) = stream.next_notification().await {
                events.log.lock().unwrap().push(note);
            }
        });
    }

    /// Stop the engine, as when the computer shuts down.
    pub async fn stop(&mut self) {
        self.client = None;
        self.engine
            .take()
            .expect("node is running")
            .shutdown()
            .await;
    }
}

/// The whole lab: one simulated network and its computers.
pub struct Lab {
    pub net: SimNet,
    registry: Arc<Registry>,
    nodes: BTreeMap<String, Node>,
}

impl Lab {
    pub fn new() -> Self {
        Self {
            net: SimNet::new(),
            registry: Arc::new(Registry::default()),
            nodes: BTreeMap::new(),
        }
    }

    /// Start a computer called `name` with default settings.
    pub async fn add(&mut self, name: &str) {
        self.add_with(name, Setup::default()).await;
    }

    pub async fn add_with(&mut self, name: &str, mut setup: Setup) {
        let port = free_port();
        let dir = tempfile::tempdir().unwrap();
        setup.config.advanced.port = u32::from(port);
        mouser_core::config::save(dir.path(), &setup.config);
        let control_path = if cfg!(windows) {
            let n = PIPES.fetch_add(1, Ordering::Relaxed);
            PathBuf::from(format!(r"\\.\pipe\mouser-acc-{}-{n}", std::process::id()))
        } else {
            dir.path().join("mouser.sock")
        };
        let sim = self
            .net
            .add_node(name, SocketAddr::from(([127, 0, 0, 1], port)));
        sim.bind_platform(&setup.platform);
        let mut node = Node {
            name: name.to_string(),
            platform: setup.platform.clone(),
            sim,
            dir,
            events: Arc::new(Events::default()),
            setup,
            control_path,
            engine: None,
            client: None,
        };
        node.start(&self.registry).await;
        assert!(
            self.nodes.insert(name.to_string(), node).is_none(),
            "{name} added twice"
        );
    }

    /// Restart a computer: stop its engine and start a new one over the same
    /// directory and virtual OS.
    pub async fn restart(&mut self, name: &str) {
        let registry = self.registry.clone();
        let node = self.node(name);
        if node.engine.is_some() {
            node.stop().await;
        }
        // The old endpoint's socket is released by its driver task a tick after
        // shutdown; a real restart is a new process and never races.
        tokio::time::sleep(Duration::from_millis(100)).await;
        node.start(&registry).await;
    }

    /// A desk called `desk` of the computers `creator` (which creates it) and
    /// `joiners` (which join with a one-click approval), each started with its
    /// setup; returns once every one is connected to all the others and the
    /// whole layout is known.
    pub async fn form_desk(
        &mut self,
        desk: &str,
        creator: &str,
        joiners: &[&str],
        setups: Vec<(&str, Setup)>,
    ) {
        for (name, setup) in setups {
            self.add_with(name, setup).await;
        }
        let _: mouser_proto::control::Desk = self
            .node(creator)
            .call(Method::DeskCreate, json!({ "name": desk }))
            .await;
        for joiner in joiners {
            let target = self.node(creator).sim.clone();
            let address = self.node(joiner).sim.route_to(&target).to_string();
            let _: mouser_proto::control::JoinPending = self
                .node(joiner)
                .call(Method::DeskJoin, json!({ "address": address }))
                .await;
            let value = self
                .node(creator)
                .notification("a join request", Topic::JoinRequest, |p| {
                    p["computer"]["name"] == *joiner
                })
                .await;
            let request: mouser_proto::control::PendingJoin =
                serde_json::from_value(value).unwrap();
            let _: mouser_proto::control::Empty = self
                .node(creator)
                .call(
                    Method::JoinDecide,
                    json!({"request_id": request.request_id, "approve": true}),
                )
                .await;
        }
        let all = joiners.len() + 1;
        for name in std::iter::once(&creator).chain(joiners) {
            let node = self.node(name);
            node.wait_status("everyone connected with their displays", |s| {
                s.computers.len() == all
                    && s.computers.iter().all(|c| {
                        c.connection == mouser_proto::control::ConnectionState::SelfComputer
                            || (c.connection == mouser_proto::control::ConnectionState::Connected
                                && c.displays > 0)
                    })
            })
            .await;
            let end = Instant::now() + WAIT;
            while node.layout().placements.len() != all {
                assert!(
                    Instant::now() < end,
                    "{name}: timed out waiting for the layout"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }

    pub fn node(&mut self, name: &str) -> &mut Node {
        self.nodes
            .get_mut(name)
            .unwrap_or_else(|| panic!("no computer called {name}"))
    }

    pub fn names(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    /// Stop every engine.
    pub async fn shutdown(&mut self) {
        for node in self.nodes.values_mut() {
            if node.engine.is_some() {
                node.stop().await;
            }
        }
    }
}
