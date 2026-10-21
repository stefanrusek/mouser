//! Helpers for unit tests that run real engines on loopback: an in-process
//! discovery registry and a way to start named nodes and call their API.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mouser_proto::control::{Method, RpcError};
use mouser_proto::ids::ComputerId;
use mouser_testkit::VirtualPlatform;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::discovery::{Advert, Discovery, Found};
use crate::engine::{Engine, EngineParams};

/// In-process stand-in for the network's mDNS.
#[derive(Default)]
pub(crate) struct Registry {
    adverts: Mutex<BTreeMap<ComputerId, Advert>>,
}

impl Registry {
    pub fn advert_of(&self, computer: ComputerId) -> Option<Advert> {
        self.adverts.lock().unwrap().get(&computer).cloned()
    }
}

pub(crate) struct Browser {
    registry: Arc<Registry>,
    own: Mutex<Option<ComputerId>>,
}

impl Discovery for Browser {
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
        let adverts = self.registry.adverts.lock().unwrap();
        adverts
            .values()
            .filter(|a| Some(a.computer) != own)
            .map(|a| Found {
                computer: a.computer,
                display_name: a.display_name.clone(),
                protocol: a.protocol,
                desk: a.desk.clone(),
                addrs: vec![SocketAddr::from(([127, 0, 0, 1], a.port))],
            })
            .collect()
    }

    fn stop(&self) {
        let own = self.own.lock().unwrap().take();
        self.registry
            .adverts
            .lock()
            .unwrap()
            .retain(|id, _| Some(*id) != own);
    }
}

pub(crate) struct Lab {
    pub registry: Arc<Registry>,
}

pub(crate) struct TestNode {
    pub engine: Engine,
    pub platform: VirtualPlatform,
    pub dir: tempfile::TempDir,
    pub control_path: std::path::PathBuf,
}

pub(crate) fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

impl Lab {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(Registry::default()),
        }
    }

    pub fn browser(&self) -> Arc<dyn Discovery> {
        Arc::new(Browser {
            registry: self.registry.clone(),
            own: Mutex::new(None),
        })
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
        let platform = VirtualPlatform::new();
        let control_path = control_path(&dir);
        let mut params = EngineParams::new(
            Arc::new(platform.clone()),
            dir.path().to_path_buf(),
            control_path.clone(),
            name.to_string(),
            "9.9.9".to_string(),
        );
        params.protocol_version = protocol;
        params.discovery = Some(self.browser());
        TestNode {
            engine: Engine::start(params).await,
            platform,
            dir,
            control_path,
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
        let control_path = control_path(&dir);
        let mut params = EngineParams::new(
            Arc::new(platform.clone()),
            dir.path().to_path_buf(),
            control_path.clone(),
            name.to_string(),
            "0.1.0".to_string(),
        );
        params.protocol_version = protocol;
        params.discovery = Some(self.browser());
        TestNode {
            engine: Engine::start(params).await,
            platform,
            dir,
            control_path,
        }
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
    let address = format!("127.0.0.1:{}", member.engine.local_addr().port());
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
        let mut link = crate::transport::Link::open(conn).await;
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
