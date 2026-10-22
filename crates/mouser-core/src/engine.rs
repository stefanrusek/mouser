//! `Engine`: startup and shutdown, composing identity, state, config,
//! discovery, transport, desk and control modules.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use mouser_platform::{Platform, SecretStore};
use mouser_proto::config::Config;
use mouser_proto::control::Notification;
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::{DeskState, DisplayId, Layout, Member};
use mouser_proto::wire::{ControlMessage, PROTOCOL_VERSION, PeerStatus};
use tokio::sync::{Notify, broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::clipboard::Command;
use crate::clipboard::files::Inbox;
use crate::clipboard::targeted::Chord;
use crate::config;
use crate::control::{self, ControlHandle};
use crate::crossing::buttons::Buttons45;
use crate::crossing::panic::PanicSpec;
use crate::crossing::{Crossing, Host};
use crate::desk::join::Requests;
use crate::discovery::{Advert, Discovery, MdnsDiscovery};
use crate::identity::Identity;
use crate::problems::Problems;
use crate::session::handshake::PeerHello;
use crate::state::{self, LocalState, desk_key_name};
use crate::transport::Transport;

/// How often discovery results are reconciled with connections.
const RECONCILE: Duration = Duration::from_millis(250);

/// Everything an engine needs from its host.
pub struct EngineParams {
    pub platform: Arc<dyn Platform>,
    /// Directory holding `config.toml` and `state.json`.
    pub config_dir: PathBuf,
    /// Control socket path (Unix) or pipe name (Windows).
    pub control_path: PathBuf,
    /// The operating system's name for this computer (the config can override it).
    pub computer_name: String,
    pub mouser_version: String,
    /// Wire protocol version spoken; only tests use anything but `PROTOCOL_VERSION`.
    pub protocol_version: u16,
    /// Discovery backend; `None` selects real mDNS on the physical LAN.
    pub discovery: Option<Arc<dyn Discovery>>,
}

impl EngineParams {
    pub fn new(
        platform: Arc<dyn Platform>,
        config_dir: PathBuf,
        control_path: PathBuf,
        computer_name: String,
        mouser_version: String,
    ) -> Self {
        Self {
            platform,
            config_dir,
            control_path,
            computer_name,
            mouser_version,
            protocol_version: PROTOCOL_VERSION,
            discovery: None,
        }
    }
}

/// Things the app shows that the control API's topics have no place for.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    /// The pointer arrived on this computer: show where. `x`, `y` are
    /// logical pixels of `display`; `desktop` is the same point in this
    /// computer's desktop coordinates.
    Landing {
        display: DisplayId,
        x: f32,
        y: f32,
        desktop: (f64, f64),
    },
    /// A computer's displays were placed or replaced in the layout: offer
    /// the desk owner a review of the placement.
    ReviewOffered { computer: ComputerId, name: String },
}

/// Where a connection to a peer is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// `Hello`s exchanged.
    Greeted,
    /// We sent a `JoinRequest` and wait for the decision.
    AwaitingDecision,
    /// Protocol versions cannot work together; no desk traffic.
    Incompatible,
    /// The peer proved desk membership.
    Authed,
    /// One of us left the desk: the connection is closing and nothing more
    /// is sent on it or expected from it.
    Departed,
}

pub(crate) enum Outgoing {
    Message(ControlMessage),
    /// Flush what is queued, then close the connection.
    Close,
}

pub(crate) struct Peer {
    pub link_id: u64,
    pub hello: PeerHello,
    pub conn: quinn::Connection,
    pub out: mpsc::UnboundedSender<Outgoing>,
    /// We dialled this connection (otherwise the peer did).
    pub dialed: bool,
    pub phase: Phase,
    pub displays: Vec<mouser_proto::model::Display>,
    pub status: Option<PeerStatus>,
    pub last_seen: String,
}

impl Peer {
    pub fn send(&self, msg: ControlMessage) {
        // A closed channel means the link already ended; its reader cleans up.
        let _ = self.out.send(Outgoing::Message(msg));
    }

    pub fn close(&self) {
        let _ = self.out.send(Outgoing::Close);
    }
}

/// What a pending join is for.
pub(crate) struct PendingJoin {
    pub name: Option<String>,
}

pub(crate) struct Core {
    pub config: Config,
    pub state: LocalState,
    pub desk_key: Option<[u8; 32]>,
    pub pending: Option<PendingJoin>,
    pub peers: BTreeMap<ComputerId, Peer>,
    /// Desk-mates we lost the connection to, and since when.
    pub absent: crate::session::liveness::AbsentPeers,
    /// Desk-mates to dial again after this computer's address changed.
    pub redial: BTreeSet<ComputerId>,
    /// Physical LAN addresses of this computer when last looked at.
    pub physical: BTreeSet<std::net::IpAddr>,
    pub dialing: BTreeSet<ComputerId>,
    pub requests: Requests,
    pub paused: bool,
    /// Displays whose placement the desk owner has not reviewed yet.
    pub needs_review: BTreeSet<DisplayId>,
    /// The application holding Secure Input on this computer.
    pub secure_holder: Option<mouser_platform::SecureInputHolder>,
}

/// State shared by every task of one engine.
pub(crate) struct Ctx {
    pub identity: Identity,
    pub platform: Arc<dyn Platform>,
    pub config_dir: PathBuf,
    pub computer_name: String,
    pub mouser_version: String,
    pub protocol_version: u16,
    pub transport: Transport,
    pub discovery: Arc<dyn Discovery>,
    pub problems: Arc<Problems>,
    pub notify: broadcast::Sender<Notification>,
    pub events: broadcast::Sender<EngineEvent>,
    pub crossing: Arc<Mutex<Crossing>>,
    pub secrets: Mutex<Box<dyn SecretStore>>,
    /// Tells the clipboard task what to do.
    pub clip: mpsc::UnboundedSender<Command>,
    /// Files pasted from other computers, which streams are expected for.
    pub clip_inbox: Inbox,
    /// Tells the transfer task what to do.
    pub xfer: mpsc::UnboundedSender<crate::transfer::engine::Command>,
    /// The dragged-file transfers in progress.
    pub transfer_ids: crate::transfer::engine::Ids,
    /// Dragged-file streams waiting to hear what to do.
    pub xfer_waiters: crate::transfer::Waiters,
    pub core: Mutex<Core>,
    pub next_link: AtomicU64,
    stop: CancellationToken,
    tasks: TaskTracker,
}

impl Ctx {
    pub fn me(&self) -> ComputerId {
        self.identity.computer_id()
    }

    pub fn display_name(&self) -> String {
        let core = self.core.lock().unwrap();
        self.display_name_in(&core)
    }

    pub fn display_name_in(&self, core: &Core) -> String {
        match core.config.general.computer_name.as_str() {
            "" => self.computer_name.clone(),
            name => name.to_string(),
        }
    }

    /// Run `task` until the engine shuts down.
    pub fn spawn(&self, task: impl std::future::Future<Output = ()> + Send + 'static) {
        let stop = self.stop.clone();
        self.tasks.spawn(async move {
            stop.run_until_cancelled(task).await;
        });
    }

    pub fn next_link_id(&self) -> u64 {
        self.next_link.fetch_add(1, Ordering::Relaxed)
    }

    pub fn state_path(&self) -> PathBuf {
        self.config_dir.join(state::FILE)
    }

    /// The `Hello` this computer sends first on every connection.
    pub fn hello(&self) -> ControlMessage {
        let core = self.core.lock().unwrap();
        ControlMessage::Hello {
            protocol_version: self.protocol_version,
            mouser_version: self.mouser_version.clone(),
            computer_id: self.me(),
            display_name: self.display_name_in(&core),
            os: crate::platform_os(self.platform.info().os),
            desk_id: core.state.desk.as_ref().map(|d| d.desk_id),
        }
    }

    /// This computer as a desk member.
    pub fn member_in(&self, core: &Core) -> Member {
        Member {
            computer_id: self.me(),
            device_public_key: self.identity.public_key(),
            display_name: self.display_name_in(core),
            os: crate::platform_os(self.platform.info().os),
            mouser_version: self.mouser_version.clone(),
            joined_at: crate::problems::now(),
        }
    }

    /// The initial desk state with this computer as the only member.
    pub fn new_desk_state(&self, core: &Core) -> DeskState {
        DeskState {
            epoch: 1,
            author: self.me(),
            members: vec![self.member_in(core)],
            layout: Layout {
                placements: crate::layout::autoplace::place(&[], &self.local_displays(core)),
                equalize_dimensions: core.config.crossing.equalize_dimensions,
                block_fullscreen: core.config.crossing.block_fullscreen,
                ..Layout::default()
            },
            allowlist: core.config.advanced.allowlist.clone(),
        }
    }

    /// Tell the network what this computer is and which desk it is on.
    pub fn announce(&self) {
        let advert = {
            let core = self.core.lock().unwrap();
            Advert {
                computer: self.me(),
                display_name: self.display_name_in(&core),
                protocol: self.protocol_version,
                desk: core
                    .state
                    .desk
                    .as_ref()
                    .map(|d| (d.desk_id, d.name.clone())),
                port: self.transport.local_addr().port(),
            }
        };
        self.discovery.announce(&advert);
    }

    /// Persist the state, and the desk key in the secret store.
    pub fn persist(&self, core: &Core) {
        core.state.save(&self.state_path());
    }

    pub fn store_desk_key(&self, desk: &DeskId, key: &[u8; 32]) {
        self.secrets
            .lock()
            .unwrap()
            .set(&desk_key_name(desk), key)
            .expect("secret store writable");
    }

    pub fn delete_desk_key(&self, desk: &DeskId) {
        self.secrets
            .lock()
            .unwrap()
            .delete(&desk_key_name(desk))
            .expect("secret store writable");
    }

    /// Send the current `status` to subscribers.
    pub fn notify_status(&self) {
        let status = control::status::build(self);
        let note = Notification {
            jsonrpc: "2.0".into(),
            method: mouser_proto::control::Topic::Status,
            params: serde_json::to_value(status).expect("status serializes"),
        };
        // No subscribers is fine.
        let _ = self.notify.send(note);
    }
}

/// A running engine.
pub struct Engine {
    pub(crate) ctx: Arc<Ctx>,
    control_path: PathBuf,
    watch: mouser_platform::WatchHandle,
    /// Dropping it stops watching the network.
    _network: mouser_platform::WatchHandle,
}

impl Engine {
    /// Start the engine: load identity, config and state, bind the QUIC
    /// endpoint, announce on the network, serve the control API, and resume
    /// the desk from persisted state with no action.
    pub async fn start(params: EngineParams) -> Engine {
        std::fs::create_dir_all(&params.config_dir).expect("config directory");
        let config = config::load_or_create(&params.config_dir);
        let mut secrets = params.platform.secrets();
        let identity = Identity::load_or_create(secrets.as_mut());
        let state = LocalState::load(&params.config_dir.join(state::FILE));
        let desk_key = state.desk.as_ref().map(|d| {
            let stored = secrets
                .get(&desk_key_name(&d.desk_id))
                .expect("secret store readable")
                .expect("desk key stored for the persisted desk");
            <[u8; 32]>::try_from(stored).expect("desk key is 32 bytes")
        });
        let port = u16::try_from(config.advanced.port).expect("validated port");
        let transport = Transport::bind(&identity, port);
        let discovery = params.discovery.unwrap_or_else(|| {
            Arc::new(MdnsDiscovery::new(
                identity.computer_id(),
                params.platform.interfaces(),
            ))
        });
        let (notify, _) = broadcast::channel(256);
        let (events, _) = broadcast::channel(256);
        let dirty = Arc::new(Notify::new());
        let problems = Arc::new(Problems::new(notify.clone()));
        let panic = PanicSpec::parse(&config.hotkeys.panic).expect("hotkeys.panic is valid");
        let targeted_paste =
            Chord::parse(&config.hotkeys.targeted_paste).expect("hotkeys.targeted_paste is valid");
        let (clip, clip_commands) = mpsc::unbounded_channel();
        let (xfer, xfer_commands) = mpsc::unbounded_channel();
        let crossing = Arc::new(Mutex::new(Crossing::new(
            identity.computer_id(),
            crate::platform_os(params.platform.info().os),
            Buttons45::from_config(&config.mouse.buttons_4_5),
            panic,
            targeted_paste,
            Host {
                events: events.clone(),
                dirty: dirty.clone(),
                platform: params.platform.clone(),
                problems: problems.clone(),
                clipboard: clip.clone(),
                drag: params.platform.file_drag(),
                transfers: xfer.clone(),
            },
        )));
        let interfaces = params.platform.interfaces();
        let ctx = Arc::new(Ctx {
            identity,
            platform: params.platform,
            config_dir: params.config_dir,
            computer_name: params.computer_name,
            mouser_version: params.mouser_version,
            protocol_version: params.protocol_version,
            transport,
            discovery,
            problems,
            notify,
            events,
            crossing,
            secrets: Mutex::new(secrets),
            clip,
            clip_inbox: Inbox::default(),
            xfer,
            transfer_ids: Default::default(),
            xfer_waiters: Default::default(),
            core: Mutex::new(Core {
                config,
                state,
                desk_key,
                pending: None,
                peers: BTreeMap::new(),
                absent: BTreeMap::new(),
                redial: BTreeSet::new(),
                physical: crate::transport::netchange::physical_addrs(&interfaces),
                dialing: BTreeSet::new(),
                requests: Requests::default(),
                paused: false,
                needs_review: BTreeSet::new(),
                secure_holder: None,
            }),
            next_link: AtomicU64::new(0),
            stop: CancellationToken::new(),
            tasks: TaskTracker::new(),
        });
        ctx.resume();
        ctx.raise_limitations();
        ctx.try_start_input();
        ctx.start_clipboard(clip_commands);
        ctx.start_transfers(xfer_commands);
        let (tx, rx) = std::sync::mpsc::channel();
        let watch = ctx
            .platform
            .watch_displays(tx)
            .expect("display changes can be watched");
        let watcher = ctx.clone();
        std::thread::Builder::new()
            .name("mouser-displays".into())
            .spawn(move || {
                // Ends when the watch is dropped.
                for _ in rx {
                    watcher.on_local_displays();
                }
            })
            .expect("display watcher starts");
        let (changes, changed) = tokio::sync::mpsc::unbounded_channel();
        let (net_tx, net_rx) = std::sync::mpsc::channel();
        let network = ctx
            .platform
            .watch_network(net_tx)
            .expect("network changes can be watched");
        std::thread::Builder::new()
            .name("mouser-network".into())
            .spawn(move || {
                // Ends when the watch is dropped.
                for () in net_rx {
                    // The receiver outlives the watch while the engine runs.
                    let _ = changes.send(());
                }
            })
            .expect("network watcher starts");
        ctx.spawn(network_loop(ctx.clone(), changed));
        ctx.spawn(accept_loop(ctx.clone()));
        ctx.spawn(reconcile_loop(ctx.clone()));
        ctx.spawn(status_loop(ctx.clone(), dirty));
        control::server::serve(&ctx, &params.control_path);
        Engine {
            ctx,
            control_path: params.control_path,
            watch,
            _network: network,
        }
    }

    pub fn computer_id(&self) -> ComputerId {
        self.ctx.me()
    }

    /// The address the QUIC endpoint is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.ctx.transport.local_addr()
    }

    /// In-process access to the control API (what the app's UI uses).
    pub fn control(&self) -> ControlHandle {
        ControlHandle::new(self.ctx.clone())
    }

    /// Everything the app shows that the control API has no topic for:
    /// the pointer landing here, and layout reviews.
    pub fn events(&self) -> broadcast::Receiver<EngineEvent> {
        self.ctx.events.subscribe()
    }

    /// The desk's layout (empty when this computer is not on a desk).
    pub fn layout(&self) -> Layout {
        let core = self.ctx.core.lock().unwrap();
        core.state
            .desk
            .as_ref()
            .map(|d| d.state.layout.clone())
            .unwrap_or_default()
    }

    /// Displays whose automatic placement has not been reviewed.
    pub fn needs_review(&self) -> Vec<DisplayId> {
        let core = self.ctx.core.lock().unwrap();
        core.needs_review.iter().cloned().collect()
    }

    /// Close every connection, leave the network and stop all tasks.
    pub async fn shutdown(self) {
        drop(self.watch);
        self.ctx.stop_input();
        for peer in self.ctx.core.lock().unwrap().peers.values() {
            peer.conn.close(0u32.into(), b"shutdown");
        }
        self.ctx.transport.close();
        let discovery = self.ctx.discovery.clone();
        tokio::task::spawn_blocking(move || discovery.stop())
            .await
            .expect("discovery stops");
        self.ctx.stop.cancel();
        self.ctx.tasks.close();
        self.ctx.tasks.wait().await;
        self.ctx.transport.wait_idle().await;
        control::server::cleanup(&self.control_path);
    }
}

async fn accept_loop(ctx: Arc<Ctx>) {
    while let Some(incoming) = ctx.transport.accept().await {
        let task = ctx.clone().accept_one(incoming);
        ctx.spawn(async move {
            task.await;
        });
    }
}

/// Handle each network change; changes that arrive while one is being
/// handled count as one more.
async fn network_loop(ctx: Arc<Ctx>, mut changed: mpsc::UnboundedReceiver<()>) {
    while changed.recv().await.is_some() {
        while changed.try_recv().is_ok() {}
        ctx.on_network_change();
    }
}

async fn reconcile_loop(ctx: Arc<Ctx>) {
    let mut tick = tokio::time::interval(RECONCILE);
    loop {
        tick.tick().await;
        ctx.reconcile();
        ctx.redial_pending();
        ctx.check_liveness();
        ctx.try_start_input();
        ctx.sync_secure_input();
    }
}

/// Tell `status` subscribers when crossing changed what it reports.
async fn status_loop(ctx: Arc<Ctx>, dirty: Arc<Notify>) {
    loop {
        dirty.notified().await;
        ctx.notify_status();
    }
}

impl Ctx {
    /// Dial discovered computers we should be connected to: desk-mates and
    /// computers with an incompatible protocol (so both sides are told).
    /// Only the lower `ComputerId` dials, so a pair never connects twice.
    pub fn reconcile(self: &Arc<Self>) {
        let found = self.discovery.found();
        let mut core = self.core.lock().unwrap();
        let my_desk = core.state.desk.as_ref().map(|d| d.desk_id);
        for f in found {
            let known = core.peers.contains_key(&f.computer) || core.dialing.contains(&f.computer);
            let same_desk = my_desk.is_some() && f.desk.as_ref().map(|d| d.0) == my_desk;
            let incompatible =
                !mouser_proto::wire::is_compatible(f.protocol, self.protocol_version);
            if !known && (same_desk || incompatible) && self.me() < f.computer {
                core.dialing.insert(f.computer);
                self.spawn(self.clone().dial_found(f));
            }
        }
    }
}
