use std::sync::{Arc, Mutex};

use mouser_core::ControlHandle;
use mouser_proto::config::Config;
use mouser_proto::control::EngineState;

use crate::hotkeys::{Hotkey, Hotkeys, Press, Shortcuts};
use crate::support_tests::{fixture, status_of};
use serde_json::json;

type Replacement = Vec<(String, Hotkey)>;

/// Records what the app asks the operating system to register.
#[derive(Default, Clone)]
pub struct FakeShortcuts {
    pub replaced: Arc<Mutex<Vec<Replacement>>>,
    press: Arc<Mutex<Option<Press>>>,
    /// What the pretend operating system refuses to register.
    pub refuse: Arc<Mutex<Vec<(Hotkey, String)>>>,
}

impl Shortcuts for FakeShortcuts {
    fn replace(&self, bindings: &[(String, Hotkey)], press: Press) -> Vec<(Hotkey, String)> {
        self.replaced.lock().unwrap().push(bindings.to_vec());
        *self.press.lock().unwrap() = Some(press);
        self.refuse.lock().unwrap().clone()
    }
}

impl FakeShortcuts {
    fn pressed(&self, hotkey: Hotkey) {
        let press = self.press.lock().unwrap().clone().unwrap();
        press(hotkey);
    }
}

/// Hotkeys over fake OS shortcuts, a config the test can change, and a record
/// of the hotkeys handed on.
pub struct Setup {
    pub hotkeys: Arc<Hotkeys>,
    pub shortcuts: FakeShortcuts,
    pub config: Arc<Mutex<Config>>,
    /// The hotkeys handed on to the engine, in order.
    pub forwarded: Arc<Mutex<Vec<Hotkey>>>,
}

pub fn hotkeys_for(control: ControlHandle) -> Setup {
    let shortcuts = FakeShortcuts::default();
    let config = Arc::new(Mutex::new(Config::default()));
    let forwarded = Arc::new(Mutex::new(Vec::new()));
    let (read, sink) = (config.clone(), forwarded.clone());
    let hotkeys = Hotkeys::new(
        Box::new(shortcuts.clone()),
        control,
        Box::new(move || read.lock().unwrap().clone()),
        Arc::new(move |hotkey| sink.lock().unwrap().push(hotkey)),
    );
    Setup {
        hotkeys,
        shortcuts,
        config,
        forwarded,
    }
}

struct Fixed {
    set: Setup,
    f: crate::support_tests::Fixture,
}

impl std::ops::Deref for Fixed {
    type Target = Setup;
    fn deref(&self) -> &Setup {
        &self.set
    }
}

async fn setup() -> Fixed {
    let f = fixture().await;
    Fixed {
        set: hotkeys_for(f.host.control()),
        f,
    }
}

fn binding(accelerator: &str, hotkey: Hotkey) -> (String, Hotkey) {
    (accelerator.to_string(), hotkey)
}

#[tokio::test(flavor = "multi_thread")]
async fn every_hotkey_is_registered_with_the_system() {
    let s = setup().await;
    s.hotkeys.sync();
    // Registered so the operating system keeps QuickJump and targeted paste
    // from the application in front; core acts on them from the captured
    // stream.
    let defaults = Config::default().hotkeys;
    assert_eq!(
        *s.shortcuts.replaced.lock().unwrap(),
        vec![vec![
            binding(&defaults.enable_disable, Hotkey::EnableDisable),
            binding(&defaults.quickjump, Hotkey::QuickJump),
            binding(&defaults.targeted_paste, Hotkey::TargetedPaste),
        ]]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn quickjump_and_targeted_paste_presses_are_handed_on() {
    let s = setup().await;
    s.hotkeys.sync();
    s.shortcuts.pressed(Hotkey::QuickJump);
    s.shortcuts.pressed(Hotkey::TargetedPaste);
    let mut forwarded = Vec::new();
    for _ in 0..200 {
        forwarded = s.forwarded.lock().unwrap().clone();
        if forwarded.len() == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    forwarded.sort_by_key(|h| h.name());
    assert_eq!(forwarded, vec![Hotkey::QuickJump, Hotkey::TargetedPaste]);
    assert_eq!(status_of(&s.f.host).await.state, EngineState::Active);
}

#[tokio::test(flavor = "multi_thread")]
async fn unchanged_config_leaves_the_registration_alone() {
    let s = setup().await;
    s.hotkeys.sync();
    s.hotkeys.sync();
    assert_eq!(s.shortcuts.replaced.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_in_config_registers_the_new_accelerator() {
    let s = setup().await;
    s.hotkeys.sync();
    {
        let mut config = s.config.lock().unwrap();
        config.hotkeys.enable_disable = "Ctrl+Alt+P".into();
        config.hotkeys.quickjump = "Ctrl+Alt+K".into();
        config.hotkeys.targeted_paste = "Ctrl+Alt+B".into();
    }
    s.hotkeys.sync();
    let replaced = s.shortcuts.replaced.lock().unwrap();
    assert_eq!(replaced.len(), 2);
    assert_eq!(
        replaced[1],
        vec![
            binding("Ctrl+Alt+P", Hotkey::EnableDisable),
            binding("Ctrl+Alt+K", Hotkey::QuickJump),
            binding("Ctrl+Alt+B", Hotkey::TargetedPaste),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_config_notification_for_any_hotkey_key_from_any_origin_registers_again() {
    let s = setup().await;
    s.hotkeys.sync();
    for (i, origin) in ["app", "cli", "file", "desk_mate"].into_iter().enumerate() {
        s.config.lock().unwrap().hotkeys.quickjump = format!("Ctrl+Alt+{i}");
        s.hotkeys.on_config(&json!({
            "key": "hotkeys.quickjump", "old": "x", "new": "y", "origin": origin
        }));
        assert_eq!(
            s.shortcuts.replaced.lock().unwrap().len(),
            i + 2,
            "{origin}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_config_notification_for_another_key_leaves_the_registration_alone() {
    let s = setup().await;
    s.hotkeys.sync();
    s.config.lock().unwrap().hotkeys.enable_disable = "Ctrl+Alt+P".into();
    s.hotkeys.on_config(&json!({
        "key": "advanced.port", "old": 1, "new": 2, "origin": "cli"
    }));
    assert_eq!(s.shortcuts.replaced.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hotkey_the_system_refuses_is_reported_and_nothing_panics() {
    let s = setup().await;
    assert!(s.hotkeys.refusals().is_empty());
    s.shortcuts.refuse.lock().unwrap().extend([
        (
            Hotkey::EnableDisable,
            "Ctrl+Alt+Shift+M is already used by another application".into(),
        ),
        (
            Hotkey::QuickJump,
            "Ctrl+Alt+Shift+J is already used by another application".into(),
        ),
    ]);
    s.hotkeys.sync();
    assert_eq!(
        s.hotkeys.refusals(),
        std::collections::BTreeMap::from([
            (
                "enable_disable".to_string(),
                "Ctrl+Alt+Shift+M is already used by another application".to_string()
            ),
            (
                "quickjump".to_string(),
                "Ctrl+Alt+Shift+J is already used by another application".to_string()
            ),
        ])
    );

    // A later registration that the system accepts clears the refusal.
    s.shortcuts.refuse.lock().unwrap().clear();
    s.config.lock().unwrap().hotkeys.enable_disable = "Ctrl+Alt+P".into();
    s.hotkeys.sync();
    assert!(s.hotkeys.refusals().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_enable_disable_hotkey_pauses_and_then_resumes() {
    let s = setup().await;
    s.hotkeys.sync();
    assert_eq!(status_of(&s.f.host).await.state, EngineState::Active);
    s.shortcuts.pressed(Hotkey::EnableDisable);
    let host = &s.f.host;
    let mut paused = false;
    for _ in 0..200 {
        paused = status_of(host).await.state == EngineState::Paused;
        if paused {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(paused, "the hotkey paused the engine");

    s.shortcuts.pressed(Hotkey::EnableDisable);
    let mut active = false;
    for _ in 0..200 {
        active = status_of(host).await.state == EngineState::Active;
        if active {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(active, "the hotkey resumed the engine");
}

#[test]
fn each_hotkey_has_its_config_name() {
    assert_eq!(Hotkey::EnableDisable.name(), "enable_disable");
    assert_eq!(Hotkey::QuickJump.name(), "quickjump");
    assert_eq!(Hotkey::TargetedPaste.name(), "targeted_paste");
}
