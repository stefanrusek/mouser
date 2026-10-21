//! The engine host: the app process owns one `mouser_core::Engine` and hands
//! the UI an in-process control-API client.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mouser_core::{ControlHandle, Engine, EngineParams};
use mouser_platform::Platform;

/// The running engine and the platform it runs on.
pub struct Host {
    control: ControlHandle,
    // Kept alive for the life of the app. `Engine` is `Send` but not `Sync`,
    // and Tauri state must be both.
    _engine: Mutex<Engine>,
    platform: Arc<dyn Platform>,
}

impl Host {
    /// Start an engine on `platform`.
    pub async fn start(
        platform: Arc<dyn Platform>,
        config_dir: PathBuf,
        control_path: PathBuf,
        computer_name: String,
        version: String,
    ) -> Self {
        let engine = Engine::start(EngineParams::new(
            platform.clone(),
            config_dir,
            control_path,
            computer_name,
            version,
        ))
        .await;
        Self {
            control: engine.control(),
            _engine: Mutex::new(engine),
            platform,
        }
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
