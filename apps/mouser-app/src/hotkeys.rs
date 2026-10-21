//! Global hotkeys: the accelerators in config (`hotkeys.*`) registered with
//! the operating system. The registration itself is behind [`Shortcuts`] so
//! the logic here runs without a live app.

use std::sync::{Arc, Mutex};

use mouser_core::ControlHandle;
use mouser_proto::config::Config;
use mouser_proto::control::{EngineState, Method};

use crate::tray::current_status;

/// What a hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// Pause crossing, or resume it (Story 2 sc. 12).
    EnableDisable,
    QuickJump,
    TargetedPaste,
}

impl Hotkey {
    /// The hotkey's name in the `hotkeys` section of config.
    pub fn name(self) -> &'static str {
        match self {
            Self::EnableDisable => "enable_disable",
            Self::QuickJump => "quickjump",
            Self::TargetedPaste => "targeted_paste",
        }
    }
}

/// Called when the owner presses a registered accelerator.
pub type Press = Arc<dyn Fn(Hotkey) + Send + Sync>;

/// The operating system's global shortcuts.
pub trait Shortcuts: Send + Sync {
    /// Replace every registered shortcut with `bindings` (accelerator and
    /// what it does); each press calls `press`.
    fn replace(&self, bindings: &[(String, Hotkey)], press: Press);
}

type Binding = (String, Hotkey);

pub struct Hotkeys {
    shortcuts: Box<dyn Shortcuts>,
    control: ControlHandle,
    config: Box<dyn Fn() -> Config + Send + Sync>,
    forward: Press,
    registered: Mutex<Vec<Binding>>,
}

impl Hotkeys {
    /// `config` reads the current configuration; `forward` receives the
    /// hotkeys the engine acts on itself rather than through the control API.
    pub fn new(
        shortcuts: Box<dyn Shortcuts>,
        control: ControlHandle,
        config: Box<dyn Fn() -> Config + Send + Sync>,
        forward: Press,
    ) -> Arc<Self> {
        Arc::new(Self {
            shortcuts,
            control,
            config,
            forward,
            registered: Mutex::new(Vec::new()),
        })
    }

    /// Register the accelerators in config, unless they are already registered.
    pub fn sync(self: &Arc<Self>) {
        let hotkeys = (self.config)().hotkeys;
        let wanted = vec![
            (hotkeys.enable_disable, Hotkey::EnableDisable),
            (hotkeys.quickjump, Hotkey::QuickJump),
            (hotkeys.targeted_paste, Hotkey::TargetedPaste),
        ];
        let mut registered = self.registered.lock().unwrap();
        if *registered == wanted {
            return;
        }
        let me = self.clone();
        self.shortcuts.replace(
            &wanted,
            Arc::new(move |hotkey| {
                let me = me.clone();
                tauri::async_runtime::spawn(async move { me.press(hotkey).await });
            }),
        );
        *registered = wanted;
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
            other => (self.forward)(other),
        }
    }
}
