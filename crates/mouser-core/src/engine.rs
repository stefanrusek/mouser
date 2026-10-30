//! `Engine`: startup and shutdown, composing identity, state, config,
//! discovery, transport, desk and control modules.

use crate::lock::LockExt;
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use mouser_platform::{Platform, PlatformProblem, SecretStore};
use mouser_proto::config::Config;
use mouser_proto::control::Notification;
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::{DeskState, DisplayId, Layout, Member};
use mouser_proto::wire::{ComputerMode, ControlMessage, PROTOCOL_VERSION, PeerStatus, TransferId};
use tokio::sync::{Notify, broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::clipboard::Command;
use crate::clipboard::files::Inbox;
use crate::config;
use crate::control::{self, ControlHandle};
use crate::crossing::buttons::Buttons45;
use crate::crossing::{Crossing, Host};
use crate::desk::join::Requests;
use crate::discovery::{Advert, Discovery, MdnsDiscovery};
use crate::faults::{self, RecoveryAction, run_unit};
use crate::identity::Identity;
use crate::logging::{self, Log, events};
use crate::problems::Problems;
use crate::release::http::Http;
use crate::release::pubkey::UPDATER_PUBKEY;
use crate::release::serve::FOLDER;
use crate::release::{CheckSchedule, Installer, ReleaseState, Version};
use crate::session::close_reason::{CloseReason, LinkLossReason};
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
    /// The app's installer for verified update bundles; `None` for a host
    /// that cannot install them.
    pub installer: Option<Installer>,
    /// The minisign public key releases must be signed with.
    pub release_pubkey: String,
    /// How the release location is reached.
    pub http: Http,
    /// When to look for releases by itself; `None` only on request
    /// (`update.check`).
    pub check_schedule: Option<CheckSchedule>,
    /// Listen here instead of on `advanced.port`, so a test never takes the
    /// port a Mouser running on the same machine holds.
    #[cfg(test)]
    pub(crate) port: Option<u16>,
    /// Where the device key and desk keys are kept; `None` selects the
    /// platform's own store. A headless engine (see
    /// [`crate::engine_headless`]) is given the machine-scope store, since
    /// no user store is open before anyone logs in.
    pub secrets: Option<Box<dyn SecretStore>>,
    /// Running from `MouserData/` (see [`crate::paths`]): nothing is installed
    /// on this computer, so start at login is never registered.
    pub portable: bool,
    /// How this computer runs; the diagnostic log's header and `status` say
    /// so.
    pub mode: ComputerMode,
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
            installer: None,
            release_pubkey: UPDATER_PUBKEY.to_string(),
            http: Http::system(),
            check_schedule: None,
            #[cfg(test)]
            port: None,
            secrets: None,
            portable: false,
            mode: ComputerMode::Installed,
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
    /// QuickJump was started anywhere on the desk: show each letter on the
    /// display it names (the ones that are this computer's).
    QuickJumpShow { letters: Vec<(DisplayId, char)> },
    /// QuickJump ended (a letter was chosen, or it was cancelled): remove the
    /// letters.
    QuickJumpHide,
    /// Restart this computer's Mouser as administrator (portable Windows,
    /// not elevated): the app runs the elevated restart sequence.
    RestartElevated,
    /// A unit of work faulted and was recovered from (`where` names it).
    Fault { r#where: String },
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
    /// A `Status` in the shape 0.2.x computers read.
    LegacyStatus(PeerStatus),
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
    /// Why this computer closed the link, when it did (otherwise the
    /// connection's own error says).
    pub closing: Option<LinkLossReason>,
    pub phase: Phase,
    pub displays: Vec<mouser_proto::model::Display>,
    pub status: Option<PeerStatus>,
    /// The peer told us it is paused (Story 2 scenario 12).
    pub paused: bool,
    pub last_seen: String,
}

impl Peer {
    pub fn send(&self, msg: ControlMessage) {
        // A closed channel means the link already ended; its reader cleans up.
        let _ = self.out.send(Outgoing::Message(msg));
    }

    /// Tell this peer our status in the shape it reads: `mode` goes only to
    /// computers that know it.
    pub fn send_status(&self, status: &PeerStatus) {
        let _ = self.out.send(if self.hello.speaks_v2() {
            Outgoing::Message(ControlMessage::Status(status.clone()))
        } else {
            Outgoing::LegacyStatus(status.clone())
        });
    }

    pub fn close(&self) {
        let _ = self.out.send(Outgoing::Close);
    }

    /// Close the connection now, for `why`.
    pub fn close_for(&mut self, why: CloseReason) {
        self.closing = Some(why.loss());
        self.conn.close(0u32.into(), why.bytes());
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
    /// How often this computer's physical LAN addresses have changed.
    pub moves: u64,
    /// Physical LAN addresses of this computer when last looked at.
    pub physical: BTreeSet<std::net::IpAddr>,
    pub dialing: BTreeSet<ComputerId>,
    pub requests: Requests,
    pub paused: bool,
    /// Displays whose placement the desk owner has not reviewed yet.
    pub needs_review: BTreeSet<DisplayId>,
    /// The application holding Secure Input on this computer.
    pub secure_holder: Option<mouser_platform::SecureInputHolder>,
    /// The status last sent to the desk-mates.
    pub status_sent: Option<PeerStatus>,
    /// Releases held and on offer.
    pub release: ReleaseState,
}

/// State shared by every task of one engine.
pub(crate) struct Ctx {
    pub identity: Identity,
    pub platform: Arc<dyn Platform>,
    pub config_dir: PathBuf,
    pub portable: bool,
    pub mode: ComputerMode,
    pub computer_name: String,
    pub mouser_version: String,
    pub protocol_version: u16,
    pub transport: Transport,
    pub discovery: Arc<dyn Discovery>,
    pub problems: Arc<Problems>,
    pub log: Arc<Log>,
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
    pub installer: Option<Installer>,
    pub release_pubkey: String,
    pub http: Http,
    /// Streams awaited as answers to `ReleaseFetch`: the stream's id, the
    /// computer asked, and where to hand the stream.
    pub release_waiters: Mutex<
        Vec<(
            TransferId,
            ComputerId,
            tokio::sync::oneshot::Sender<quinn::RecvStream>,
        )>,
    >,
    /// Desk-mates offered the release this computer holds.
    pub(crate) release_serving: Mutex<BTreeMap<ComputerId, crate::release::serve::Serving>>,
    /// Streams awaited as answers to `LogsRequest`.
    pub(crate) logs_waiters: Mutex<Vec<crate::logs::collect::Waiter>>,
    pub next_link: AtomicU64,
    /// Tests hold an incoming connection before it is greeted.
    #[cfg(test)]
    pub accept_hold: crate::session::AcceptHold,
    stop: CancellationToken,
    tasks: TaskTracker,
}

impl Ctx {
    pub fn me(&self) -> ComputerId {
        self.identity.computer_id()
    }

    pub fn display_name(&self) -> String {
        let core = self.core.guard();
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
        let core = self.core.guard();
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
            let core = self.core.guard();
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

    /// Persist the state. When `state.json` cannot be written the in-memory
    /// state is kept (and written by the next change that can be); callers
    /// do not fail because of it.
    #[track_caller]
    pub fn persist(&self, core: &Core) -> std::io::Result<()> {
        self.log.set_desk(
            core.state
                .desk
                .as_ref()
                .map(|d| (d.desk_id, d.name.clone())),
        );
        let saved = core.state.save(&self.state_path());
        if saved.is_err() {
            let at = std::panic::Location::caller();
            self.log.write(events::fault(
                "persist",
                &format!("{}:{}", at.file(), at.line()),
                "state.json could not be saved",
            ));
        }
        saved
    }

    /// A failing store is an ordinary failure (locked keychain, full disk):
    /// callers report it and leave their state alone. It must never panic
    /// while the core lock or the store lock is held, which would poison it.
    pub fn store_desk_key(&self, desk: &DeskId, key: &[u8; 32]) -> mouser_platform::Result<()> {
        self.secrets.guard().set(&desk_key_name(desk), key)
    }

    pub fn delete_desk_key(&self, desk: &DeskId) -> mouser_platform::Result<()> {
        self.secrets.guard().delete(&desk_key_name(desk))
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
    /// Dropping it stops listening for problems the platform notices.
    _problems: mouser_platform::WatchHandle,
}

impl Engine {
    /// Start the engine: load identity, config and state, bind the QUIC
    /// endpoint, announce on the network, serve the control API, and resume
    /// the desk from persisted state with no action.
    pub async fn start(params: EngineParams) -> Engine {
        std::fs::create_dir_all(&params.config_dir).expect("create the config directory (--headless uses the machine directory, normally /var/lib/mouser, which needs root)");
        let config::Loaded { config, rejection } = config::load(&params.config_dir);
        let mut secrets = params.secrets.unwrap_or_else(|| params.platform.secrets());
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
        #[cfg(test)]
        let port = params.port.unwrap_or(port);
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
        let info = params.platform.info();
        let log = Arc::new(Log::with_context(
            &params.config_dir,
            identity.computer_id(),
            match config.general.computer_name.as_str() {
                "" => params.computer_name.clone(),
                name => name.to_string(),
            },
            config.diagnostics.logging,
            config.diagnostics.max_log_mb,
            logging::Context {
                mouser_version: params.mouser_version.clone(),
                os: crate::platform_os(info.os),
                os_version: info.version,
                mode: params.mode,
                desk: state.desk.as_ref().map(|d| (d.desk_id, d.name.clone())),
            },
        ));
        let problems = Arc::new(Problems::new(notify.clone()).logged_to(log.clone()));
        let (panic, targeted_paste, quickjump) =
            config::validate(&config).expect("a loaded config is valid");
        let (clip, clip_commands) = mpsc::unbounded_channel();
        let (xfer, xfer_commands) = mpsc::unbounded_channel();
        let mut crossing = Crossing::new(
            identity.computer_id(),
            crate::platform_os(params.platform.info().os),
            Buttons45::from_config(&config.mouse.buttons_4_5),
            panic,
            targeted_paste,
            quickjump,
            Host {
                events: events.clone(),
                dirty: dirty.clone(),
                platform: params.platform.clone(),
                problems: problems.clone(),
                log: log.clone(),
                clipboard: clip.clone(),
                drag: params.platform.file_drag(),
                transfers: xfer.clone(),
            },
        );
        crossing.landing_highlight = config.crossing.landing_highlight;
        crossing.command_as_control = config.keyboard.command_as_control;
        let crossing = Arc::new(Mutex::new(crossing));
        let interfaces = params.platform.interfaces();
        let release = ReleaseState::load(
            &params.config_dir.join(FOLDER),
            identity.computer_id(),
            &Version::parse(&params.mouser_version).expect("mouser_version is a version"),
        );
        let ctx = Arc::new(Ctx {
            identity,
            platform: params.platform,
            portable: params.portable,
            mode: params.mode,
            config_dir: params.config_dir,
            computer_name: params.computer_name,
            mouser_version: params.mouser_version,
            protocol_version: params.protocol_version,
            transport,
            discovery,
            problems,
            log,
            notify,
            events,
            crossing,
            secrets: Mutex::new(secrets),
            clip,
            clip_inbox: Inbox::default(),
            xfer,
            transfer_ids: Default::default(),
            xfer_waiters: Default::default(),
            installer: params.installer,
            release_pubkey: params.release_pubkey,
            http: params.http,
            release_waiters: Mutex::new(Vec::new()),
            release_serving: Mutex::new(BTreeMap::new()),
            logs_waiters: Mutex::new(Vec::new()),
            core: Mutex::new(Core {
                config,
                state,
                desk_key,
                pending: None,
                peers: BTreeMap::new(),
                absent: BTreeMap::new(),
                redial: BTreeSet::new(),
                moves: 0,
                physical: crate::transport::netchange::physical_addrs(&interfaces),
                dialing: BTreeSet::new(),
                requests: Requests::default(),
                paused: false,
                needs_review: BTreeSet::new(),
                secure_holder: None,
                status_sent: None,
                release,
            }),
            next_link: AtomicU64::new(0),
            #[cfg(test)]
            accept_hold: Default::default(),
            stop: CancellationToken::new(),
            tasks: TaskTracker::new(),
        });
        ctx.resume();
        // The rejected file stays as the text already looked at: the first
        // look of the reload loop must not say it again.
        let mut seen = String::new();
        if let Some(rejection) = rejection {
            ctx.reject_config(&rejection.reason);
            seen = rejection.text;
        }
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
        let (problem_tx, problem_rx) = std::sync::mpsc::channel();
        let problems = ctx
            .platform
            .watch_problems(problem_tx)
            .expect("platform problems can be watched");
        let reporter = ctx.clone();
        std::thread::Builder::new()
            .name("mouser-platform-problems".into())
            .spawn(move || {
                // Ends when the watch is dropped.
                for problem in problem_rx {
                    match problem {
                        PlatformProblem::Raised(text) => reporter.raise_platform_problem(text),
                        PlatformProblem::Cleared(text) => reporter.clear_platform_problem(&text),
                    }
                }
            })
            .expect("platform problem watcher starts");
        ctx.spawn(network_loop(ctx.clone(), changed));
        ctx.spawn(accept_loop(ctx.clone()));
        ctx.spawn(reconcile_loop(ctx.clone(), seen));
        ctx.spawn(status_loop(ctx.clone(), dirty));
        if let Some(schedule) = params.check_schedule {
            ctx.spawn(control::update::check_loop(ctx.clone(), schedule));
        }
        control::server::serve(&ctx, &params.control_path);
        Engine {
            ctx,
            control_path: params.control_path,
            watch,
            _network: network,
            _problems: problems,
        }
    }

    pub fn computer_id(&self) -> ComputerId {
        self.ctx.me()
    }

    /// The address the QUIC endpoint is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.ctx.transport.local_addr()
    }

    /// The diagnostic log, for front ends that add lines of their own
    /// (a platform backend's debug diagnostics).
    pub fn log(&self) -> Arc<Log> {
        self.ctx.log.clone()
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
        let core = self.ctx.core.guard();
        core.state
            .desk
            .as_ref()
            .map(|d| d.state.layout.clone())
            .unwrap_or_default()
    }

    /// Displays whose automatic placement has not been reviewed.
    pub fn needs_review(&self) -> Vec<DisplayId> {
        let core = self.ctx.core.guard();
        core.needs_review.iter().cloned().collect()
    }

    /// Close every connection, leave the network and stop all tasks. In this
    /// order: stop input (the pointer is shown again, what is held is
    /// released and the computer being driven is told to let go), then
    /// close the links.
    pub async fn shutdown(self) {
        drop(self.watch);
        self.ctx.stop_input();
        // Every link ends here and now, so each is also ended and logged
        // here: the link tasks that would notice are about to be stopped.
        let links: Vec<(ComputerId, u64)> = self
            .ctx
            .core
            .guard()
            .peers
            .iter_mut()
            .map(|(id, peer)| {
                peer.close_for(CloseReason::Quit);
                (*id, peer.link_id)
            })
            .collect();
        for (id, link) in links {
            self.ctx.on_link_closed(id, link);
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
        self.ctx.transport.shutdown().await;
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

async fn reconcile_loop(ctx: Arc<Ctx>, mut config_text: String) {
    let mut tick = tokio::time::interval(RECONCILE);
    // The first look at the file finds out whether it was edited since start
    // (`config_text` starts empty unless the file was rejected at start).
    loop {
        tick.tick().await;
        // A fault in one tick returns control here and the loop carries on.
        let ticked = run_unit(&ctx.events, "reconcile", || {
            ctx.reload_config(&mut config_text);
            ctx.reconcile();
            ctx.redial_pending();
            ctx.check_liveness();
            ctx.try_start_input();
            ctx.sync_secure_input();
            ctx.hand_back_elevated();
            if ctx.sync_permission_problems() {
                ctx.notify_status();
            }
            ctx.sync_peer_status();
        });
        if ticked.is_err() {
            ctx.crossing.guard().recover_after_fault();
            faults::recovered(&ctx.log, "reconcile", RecoveryAction::ReturnedLocal);
        }
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
        let mut core = self.core.guard();
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
