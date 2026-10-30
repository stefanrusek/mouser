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
    pub async fn connect(
        path: &Path,
        platform: Arc<dyn Platform>,
        log_dir: &Path,
    ) -> std::io::Result<Self> {
        let remote = Remote::connect(path).await?;
        let nobody = "00".repeat(16).parse().expect("an all-zero id");
        Ok(Self {
            control: Control::Remote(remote),
            log: Arc::new(Log::new(log_dir, nobody, String::new(), false, 1)),
            engine: Mutex::new(None),
            platform,
            transfers: Mutex::new(Vec::new()),
            arranger_edge: Mutex::new(None),
            toasts: Mutex::new(Toasts::default()),
            names: Mutex::new(Names::new()),
        })
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

/// A control-API client of the session engine: one connection for calls,
/// one that hears every notification. When the engine goes away its
/// notifications end (`Closed`), as an in-process engine's do.
#[derive(Clone)]
pub struct Remote {
    calls: Arc<tokio::sync::Mutex<ControlClient>>,
    notes: tokio::sync::broadcast::WeakSender<Notification>,
    events: tokio::sync::broadcast::Sender<EngineEvent>,
}

impl Remote {
    /// Connects to the control API at `path`.
    pub async fn connect(path: &Path) -> std::io::Result<Remote> {
        let calls = ControlClient::connect(path).await?;
        let mut listening = ControlClient::connect(path).await?;
        let topics = vec![
            Topic::Config,
            Topic::Problem,
            Topic::JoinRequest,
            Topic::Transfer,
            Topic::Status,
        ];
        listening
            .subscribe(topics)
            .await
            .expect("the engine takes a subscription to its own topics");
        let (heard, _) = tokio::sync::broadcast::channel(256);
        let notes = heard.downgrade();
        tauri::async_runtime::spawn(async move {
            while let Some(note) = listening.next_notification().await {
                // Nobody listening just now is fine.
                let _ = heard.send(note);
            }
        });
        Ok(Remote {
            calls: Arc::new(tokio::sync::Mutex::new(calls)),
            notes,
            events: tokio::sync::broadcast::channel(1).0,
        })
    }

    async fn call(&self, method: Method, params: Option<Value>) -> Result<Value, RpcError> {
        self.calls.lock().await.call(method, params).await
    }

    fn notifications(&self) -> tokio::sync::broadcast::Receiver<Notification> {
        match self.notes.upgrade() {
            Some(notes) => notes.subscribe(),
            // The engine has gone: nothing more will come.
            None => tokio::sync::broadcast::channel(1).1,
        }
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
        self.events.subscribe()
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
        let mut client = self.calls.lock().await;
        let result = client
            .call(Method::LogsCollectStream, Some(serde_json::json!({})))
            .await?;
        Ok(crate::commands::StreamedBundle {
            result: serde_json::from_value(result).expect("result matches its schema"),
            chunks: client.take_chunks(),
        })
    }
}
