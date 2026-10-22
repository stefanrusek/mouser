//! The engine host: the app process owns one `mouser_core::Engine` and hands
//! the UI an in-process control-API client.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mouser_core::release::{CheckSchedule, Installer};
use mouser_core::{ControlHandle, Engine, EngineParams};
use mouser_platform::Platform;
use serde_json::Value;

/// The running engine and the platform it runs on.
pub struct Host {
    control: ControlHandle,
    // Kept alive for the life of the app. `Engine` is `Send` but not `Sync`,
    // and Tauri state must be both.
    _engine: Mutex<Engine>,
    platform: Arc<dyn Platform>,
    /// The latest `transfer` notification of each transfer, first seen first.
    transfers: Mutex<Vec<Value>>,
    /// The problem id of the edge the arranger should open next.
    arranger_edge: Mutex<Option<String>>,
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
        updates: Updates,
    ) -> Self {
        let mut params = EngineParams::new(
            platform.clone(),
            config_dir,
            control_path,
            computer_name,
            version,
        );
        params.installer = updates.installer;
        params.check_schedule = updates.check_schedule;
        let engine = Engine::start(params).await;
        Self {
            control: engine.control(),
            _engine: Mutex::new(engine),
            platform,
            transfers: Mutex::new(Vec::new()),
            arranger_edge: Mutex::new(None),
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

    /// In-process access to the control API.
    pub fn control(&self) -> ControlHandle {
        self.control.clone()
    }

    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }
}

/// A second launch of Mouser opens the window the first one would.
pub fn on_second_launch<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let control = app.state::<Host>().control();
        crate::windows::open_home(&app, &control).await;
    });
}
