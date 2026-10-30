//! The engine host: the app process owns one `mouser_core::Engine` and hands
//! the UI an in-process control-API client; in UI mode (`ui_mode`) it owns
//! no engine and is a client of the session engine behind the service pipe.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mouser_core::logging::Log;
use mouser_core::release::{CheckSchedule, Installer};
use mouser_core::{ControlClient, ControlHandle, Engine, EngineEvent, EngineParams};
use mouser_platform::Platform;
use mouser_proto::control::{Method, Notification, RpcError, Status, Topic};
use mouser_proto::wire::ComputerMode;
use serde_json::Value;

use crate::notify::{Names, Notifier, Toasts};

/// The running engine and the platform it runs on.
pub struct Host {
    control: Control,
    log: Arc<Log>,
    // Kept alive for the life of the app. `Engine` is `Send` but not `Sync`,
    // and Tauri state must be both.
    engine: Mutex<Option<Engine>>,
    platform: Arc<dyn Platform>,
    /// The latest `transfer` notification of each transfer, first seen first.
    transfers: Mutex<Vec<Value>>,
    /// The problem id of the edge the arranger should open next.
    arranger_edge: Mutex<Option<String>>,
    /// Which problems were shown as system notifications, and when.
    toasts: Mutex<Toasts>,
    /// Each computer's name, from the latest status, for notifications.
    names: Mutex<Names>,
}

/// How this host takes part in updates. The default takes no part: no
/// installer, and no looking for releases by itself (what tests run with).
#[derive(Default)]
pub struct Updates {
    pub installer: Option<Installer>,
    pub check_schedule: Option<CheckSchedule>,
}

impl Host {
    /// Start an engine on `platform`.
    pub async fn start(
        platform: Arc<dyn Platform>,
        config_dir: PathBuf,
        control_path: PathBuf,
        computer_name: String,
        version: String,
        mode: ComputerMode,
        updates: Updates,
    ) -> Self {
        let mut params = EngineParams::new(
            platform.clone(),
            config_dir,
            control_path,
            computer_name,
            version,
        );
        params.mode = mode;
        params.installer = updates.installer;
        params.check_schedule = updates.check_schedule;
        Self::start_with(params).await
    }

    /// Start an engine from ready-made parameters.
    pub async fn start_with(params: EngineParams) -> Self {
        let platform = params.platform.clone();
        let engine = Engine::start(params).await;
        #[cfg(target_os = "windows")]
        mouser_platform_windows::set_diagnostics(Some(crate::diagnostics::debug_to(engine.log())));
        Self {
            control: Control::Local(engine.control()),
            log: engine.log(),
            engine: Mutex::new(Some(engine)),
            platform,
            transfers: Mutex::new(Vec::new()),
            arranger_edge: Mutex::new(None),
            toasts: Mutex::new(Toasts::default()),
            names: Mutex::new(Names::new()),
        }
    }

    /// Keep `transfer` (a `transfer` notification's params) as the latest word
    /// on its transfer, for a window that opens after the notification.
    pub fn remember_transfer(&self, transfer: Value) {
        let mut transfers = self.transfers.lock().unwrap();
        match transfers
            .iter_mut()
            .find(|t| t["transfer_id"] == transfer["transfer_id"])
        {
            Some(known) => *known = transfer,
            None => transfers.push(transfer),
        }
    }

    /// Show the problem in `note` through `notifier` if it needs the owner
    /// and was not shown lately.
    pub fn toast(&self, note: &Notification, notifier: &dyn Notifier) {
        let names = self.names.lock().unwrap();
        self.toasts
            .lock()
            .unwrap()
            .bridge(note, Instant::now(), &names, notifier);
    }

    /// Keep the computers' names in `status` for the notifications about them.
    pub fn remember_names(&self, status: &mouser_proto::control::Status) {
        *self.names.lock().unwrap() = status
            .computers
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect();
    }

    /// The latest notification of every transfer so far.
    pub fn transfers(&self) -> Vec<Value> {
        self.transfers.lock().unwrap().clone()
    }

    /// The arranger should open the edge named by this `crossing_blocked` problem id.
    pub fn ask_for_edge(&self, problem_id: String) {
        *self.arranger_edge.lock().unwrap() = Some(problem_id);
    }

    /// The edge asked for, once.
    pub fn take_edge(&self) -> Option<String> {
        self.arranger_edge.lock().unwrap().take()
    }

    /// Copy what a headless engine needs into `dir` and `machine`
    /// ([`Engine::export_machine_scope`]).
    pub fn export_machine_scope(
        &self,
        dir: &std::path::Path,
        machine: &mut dyn mouser_platform::SecretStore,
    ) {
        self.engine
            .lock()
            .unwrap()
            .as_ref()
            .expect("the engine is running")
            .export_machine_scope(dir, machine);
    }

    /// Stop the engine: input first, then the links. The first call does it;
    /// later calls (tray Quit, then the exit event) find it done.
    pub async fn shutdown(&self) {
        let engine = self.engine.lock().unwrap().take();
        if let Some(engine) = engine {
            engine.shutdown().await;
        }
    }

    /// UI mode: a host with no engine of its own, talking to the engine whose
    /// control API is at `path` (the session engine, behind the service
    /// pipe), on this computer's `platform`. Its own log, in `log_dir`, is
    /// off: the session engine logs.
    pub fn connect(path: &Path, platform: Arc<dyn Platform>, log_dir: &Path) -> Self {
        let remote = Remote::connect(path);
        let nobody = "00".repeat(16).parse().expect("an all-zero id");
        Self {
            control: Control::Remote(remote),
            log: Arc::new(Log::new(log_dir, nobody, String::new(), false, 1)),
            engine: Mutex::new(None),
            platform,
            transfers: Mutex::new(Vec::new()),
            arranger_edge: Mutex::new(None),
            toasts: Mutex::new(Toasts::default()),
            names: Mutex::new(Names::new()),
        }
    }

    /// The control API: in-process, or the session engine's in UI mode.
    pub fn control(&self) -> Control {
        self.control.clone()
    }

    /// The engine's diagnostic log.
    pub fn log(&self) -> Arc<Log> {
        self.log.clone()
    }

    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }
}

/// A second launch of Mouser brings the first one's window to the desk.
pub fn on_second_launch<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    crate::windows::navigate(app, crate::windows::DESK);
}

/// The control API this app talks to.
#[derive(Clone)]
pub enum Control {
    /// Its own engine's, in-process.
    Local(ControlHandle),
    /// In UI mode, the session engine's behind the service pipe.
    Remote(Remote),
}

impl Control {
    pub async fn call(&self, method: Method, params: Option<Value>) -> Result<Value, RpcError> {
        match self {
            Control::Local(local) => local.call(method, params).await,
            Control::Remote(remote) => remote.call(method, params).await,
        }
    }

    /// Every notification from now on (the caller filters by topic).
    pub fn notifications(&self) -> tokio::sync::broadcast::Receiver<Notification> {
        match self {
            Control::Local(local) => local.notifications(),
            Control::Remote(remote) => remote.notifications(),
        }
    }

    /// Whether the engine is there to talk to: always for an app's own
    /// engine; in UI mode, as the session engine comes and goes.
    pub fn ready(&self) -> tokio::sync::watch::Receiver<bool> {
        match self {
            Control::Local(_) => tokio::sync::watch::channel(true).1,
            Control::Remote(remote) => remote.ready(),
        }
    }

    pub fn events(&self) -> tokio::sync::broadcast::Receiver<EngineEvent> {
        match self {
            Control::Local(local) => local.events(),
            Control::Remote(remote) => remote.events(),
        }
    }

    pub fn fix(&self, problem_id: &str) -> Result<(), String> {
        match self {
            Control::Local(local) => local.fix(problem_id),
            Control::Remote(remote) => remote.fix(problem_id),
        }
    }

    pub fn resolve_transfer(&self, transfer_id: &str, overwrite: bool) -> Result<(), String> {
        match self {
            Control::Local(local) => {
                local.resolve_transfer(transfer_id, overwrite);
                Ok(())
            }
            Control::Remote(remote) => remote.resolve_transfer(transfer_id, overwrite),
        }
    }
}

/// What the session engine cannot do for the UI over the control API yet.
pub const NOT_YET: &str = "not available while Mouser runs as a service yet";

/// What a call says while the session engine is not there.
pub const NOT_READY: &str = "the session engine is not ready";

/// How long the UI waits before looking for the session engine again: the
/// service starts it again after a user switch or a service restart.
const RETRY: std::time::Duration = std::time::Duration::from_millis(500);

/// Every topic, which the UI hears all of.
const TOPICS: [Topic; 5] = [
    Topic::Config,
    Topic::Problem,
    Topic::JoinRequest,
    Topic::Transfer,
    Topic::Status,
];

/// A control-API client of the session engine: one connection for calls,
/// one that hears every notification. It survives the engine going away and
/// coming back (a user switch, a service restart): while the engine is not
/// there it is not ready and calls say so, and it connects again, and hears
/// every topic again, once the engine is back (T533, T534).
#[derive(Clone)]
pub struct Remote {
    shared: Arc<Shared>,
}

struct Shared {
    path: PathBuf,
    calls: tokio::sync::Mutex<Option<ControlClient>>,
    notes: tokio::sync::broadcast::Sender<Notification>,
    events: tokio::sync::broadcast::Sender<EngineEvent>,
    ready: tokio::sync::watch::Sender<bool>,
}

impl Remote {
    /// The client of the control API at `path`, connecting when the engine
    /// is there; never an error.
    pub fn connect(path: &Path) -> Remote {
        let shared = Arc::new(Shared {
            path: path.to_path_buf(),
            calls: tokio::sync::Mutex::new(None),
            notes: tokio::sync::broadcast::channel(256).0,
            events: tokio::sync::broadcast::channel(1).0,
            ready: tokio::sync::watch::channel(false).0,
        });
        tauri::async_runtime::spawn(listen(Arc::downgrade(&shared)));
        Remote { shared }
    }

    /// Whether the session engine is there; it changes as it comes and goes.
    pub fn ready(&self) -> tokio::sync::watch::Receiver<bool> {
        self.shared.ready.subscribe()
    }

    async fn call(&self, method: Method, params: Option<Value>) -> Result<Value, RpcError> {
        let mut calls = self.shared.calls.lock().await;
        if calls.is_none() {
            *calls = ControlClient::connect(&self.shared.path).await.ok();
        }
        let Some(client) = calls.as_mut() else {
            return Err(RpcError {
                code: -32603,
                message: NOT_READY.into(),
                data: None,
            });
        };
        let result = client.call(method, params).await;
        if result
            .as_ref()
            .is_err_and(|e| e.message == mouser_core::control::client::GONE)
        {
            // Connected again on the next call.
            *calls = None;
        }
        result
    }

    fn notifications(&self) -> tokio::sync::broadcast::Receiver<Notification> {
        self.shared.notes.subscribe()
    }

    // v2: replaced by T532 (problem.fix over the control API).
    fn fix(&self, _problem_id: &str) -> Result<(), String> {
        Err(NOT_YET.into())
    }

    // v2: replaced by T532 (transfer.resolve over the control API).
    fn resolve_transfer(&self, _transfer_id: &str, _overwrite: bool) -> Result<(), String> {
        Err(NOT_YET.into())
    }

    // v2: replaced by T532 (the engine_event topic): no engine event reaches
    // this app yet.
    fn events(&self) -> tokio::sync::broadcast::Receiver<EngineEvent> {
        self.shared.events.subscribe()
    }
}

/// Hears the session engine's notifications for as long as the client is
/// kept: connects and subscribes to every topic, hands each notification on,
/// and when the engine goes, waits for it to come back.
async fn listen(shared: std::sync::Weak<Shared>) {
    while let Some(path) = shared.upgrade().map(|s| s.path.clone()) {
        if let Ok(mut client) = ControlClient::connect(&path).await
            && client.subscribe(TOPICS.to_vec()).await.is_ok()
        {
            set_ready(&shared, true);
            while let Some(note) = client.next_notification().await {
                if let Some(shared) = shared.upgrade() {
                    // Nobody listening just now is fine.
                    let _ = shared.notes.send(note);
                }
            }
            set_ready(&shared, false);
        }
        tokio::time::sleep(RETRY).await;
    }
}

fn set_ready(shared: &std::sync::Weak<Shared>, ready: bool) {
    if let Some(shared) = shared.upgrade() {
        shared.ready.send_replace(ready);
    }
}
impl crate::commands::LogsLink for Remote {
    async fn status(&self) -> Result<Status, RpcError> {
        let status = self.call(Method::Status, None).await?;
        Ok(serde_json::from_value(status).expect("status matches its schema"))
    }

    async fn collect(&self) -> Result<Value, RpcError> {
        self.call(Method::LogsCollect, Some(serde_json::json!({})))
            .await
    }

    async fn collect_stream(&self) -> Result<crate::commands::StreamedBundle, RpcError> {
        // Through call, so a missing engine is reported as for any call.
        let result = self
            .call(Method::LogsCollectStream, Some(serde_json::json!({})))
            .await?;
        let chunks = self
            .shared
            .calls
            .lock()
            .await
            .as_mut()
            .map(ControlClient::take_chunks)
            .unwrap_or_default();
        Ok(crate::commands::StreamedBundle {
            result: serde_json::from_value(result).expect("result matches its schema"),
            chunks,
        })
    }
}
