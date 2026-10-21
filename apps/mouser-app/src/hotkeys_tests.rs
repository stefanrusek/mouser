use std::sync::{Arc, Mutex};

use mouser_core::ControlHandle;
use mouser_proto::config::Config;
use mouser_proto::control::EngineState;

use crate::hotkeys::{Hotkey, Hotkeys, Press, Shortcuts};
use crate::support_tests::{eventually, fixture, status_of};

type Replacement = Vec<(String, Hotkey)>;

/// Records what the app asks the operating system to register.
#[derive(Default, Clone)]
pub struct FakeShortcuts {
    pub replaced: Arc<Mutex<Vec<Replacement>>>,
    press: Arc<Mutex<Option<Press>>>,
}

impl Shortcuts for FakeShortcuts {
    fn replace(&self, bindings: &[(String, Hotkey)], press: Press) {
        self.replaced.lock().unwrap().push(bindings.to_vec());
        *self.press.lock().unwrap() = Some(press);
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
async fn the_three_accelerators_from_config_are_registered_with_the_system() {
    let s = setup().await;
    s.hotkeys.sync();
    assert_eq!(
        *s.shortcuts.replaced.lock().unwrap(),
        vec![vec![
            binding("Ctrl+Alt+Shift+M", Hotkey::EnableDisable),
            binding("Ctrl+Alt+Shift+J", Hotkey::QuickJump),
            binding("Ctrl+Alt+Shift+V", Hotkey::TargetedPaste),
        ]]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unchanged_config_leaves_the_registration_alone() {
    let s = setup().await;
    s.hotkeys.sync();
    s.hotkeys.sync();
    assert_eq!(s.shortcuts.replaced.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_in_config_registers_the_new_accelerators() {
    let s = setup().await;
    s.hotkeys.sync();
    {
        let mut config = s.config.lock().unwrap();
        config.hotkeys.quickjump = "Ctrl+Alt+K".into();
        config.hotkeys.enable_disable = "Ctrl+Alt+P".into();
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

#[tokio::test(flavor = "multi_thread")]
async fn quickjump_and_targeted_paste_are_handed_on() {
    let s = setup().await;
    s.hotkeys.sync();
    s.shortcuts.pressed(Hotkey::QuickJump);
    s.shortcuts.pressed(Hotkey::TargetedPaste);
    eventually(|| s.forwarded.lock().unwrap().len() == 2).await;
    assert_eq!(
        *s.forwarded.lock().unwrap(),
        vec![Hotkey::QuickJump, Hotkey::TargetedPaste]
    );
}

#[test]
fn each_hotkey_has_a_name_for_the_ui() {
    assert_eq!(Hotkey::EnableDisable.name(), "enable_disable");
    assert_eq!(Hotkey::QuickJump.name(), "quickjump");
    assert_eq!(Hotkey::TargetedPaste.name(), "targeted_paste");
}
