//! The global hotkey the operating system must see: `hotkeys.enable_disable`.
//! (QuickJump, targeted paste and the panic key are detected by core in the
//! captured input stream.) The registration itself is behind [`Shortcuts`] so
//! the logic here runs without a live app.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use mouser_core::ControlHandle;
use mouser_proto::config::Config;
use mouser_proto::control::{EngineState, Method};
use serde_json::Value;

use crate::tray::current_status;

/// What a hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// Pause crossing, or resume it (Story 2 sc. 12).
    EnableDisable,
}

impl Hotkey {
    /// The hotkey's name in the `hotkeys` section of config.
    pub fn name(self) -> &'static str {
        match self {
            Self::EnableDisable => "enable_disable",
        }
    }
}

/// Called when the owner presses a registered accelerator.
pub type Press = Arc<dyn Fn(Hotkey) + Send + Sync>;

/// The operating system's global shortcuts.
pub trait Shortcuts: Send + Sync {
    /// Replace every registered shortcut with `bindings` (accelerator and
    /// what it does); each press calls `press`. Returns the hotkeys the
    /// system refused, with its reason.
    fn replace(&self, bindings: &[(String, Hotkey)], press: Press) -> Vec<(Hotkey, String)>;
}

type Binding = (String, Hotkey);

pub struct Hotkeys {
    shortcuts: Box<dyn Shortcuts>,
    control: ControlHandle,
    config: Box<dyn Fn() -> Config + Send + Sync>,
    registered: Mutex<Vec<Binding>>,
    refused: Mutex<Vec<(Hotkey, String)>>,
}

impl Hotkeys {
    /// `config` reads the current configuration.
    pub fn new(
        shortcuts: Box<dyn Shortcuts>,
        control: ControlHandle,
        config: Box<dyn Fn() -> Config + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new(Self {
            shortcuts,
            control,
            config,
            registered: Mutex::new(Vec::new()),
            refused: Mutex::new(Vec::new()),
        })
    }

    /// Register the accelerators in config, unless they are already registered.
    pub fn sync(self: &Arc<Self>) {
        let hotkeys = (self.config)().hotkeys;
        let wanted = vec![(hotkeys.enable_disable, Hotkey::EnableDisable)];
        let mut registered = self.registered.lock().unwrap();
        if *registered == wanted {
            return;
        }
        let me = self.clone();
        let refused = self.shortcuts.replace(
            &wanted,
            Arc::new(move |hotkey| {
                let me = me.clone();
                tauri::async_runtime::spawn(async move { me.press(hotkey).await });
            }),
        );
        *registered = wanted;
        *self.refused.lock().unwrap() = refused;
    }

    /// Register again after a `config` notification (any origin) for a
    /// `hotkeys.*` key.
    pub fn on_config(self: &Arc<Self>, params: &Value) {
        let key = params["key"]
            .as_str()
            .expect("a config notification has a key");
        if key.starts_with("hotkeys.") {
            self.sync();
        }
    }

    /// The hotkeys the operating system refused, by name, with its reason.
    pub fn refusals(&self) -> BTreeMap<String, String> {
        self.refused
            .lock()
            .unwrap()
            .iter()
            .map(|(hotkey, reason)| (hotkey.name().to_string(), reason.clone()))
            .collect()
    }

    async fn press(&self, hotkey: Hotkey) {
        match hotkey {
            Hotkey::EnableDisable => {
                let method = match current_status(&self.control).await.state {
                    EngineState::Active => Method::ActivePause,
                    EngineState::Paused => Method::ActiveResume,
                };
                self.control
                    .call(method, None)
                    .await
                    .expect("the engine can be paused or resumed");
            }
        }
    }
}
