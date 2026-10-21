//! The menu-bar / system-tray model: what the icon, tooltip and menu say,
//! computed from `status`. The live tray is driven through [`TrayPort`].

use std::sync::Mutex;

use mouser_core::ControlHandle;
use mouser_core::control::status::tray_summary;
use mouser_platform::Os;
use mouser_proto::control::{EngineState, Method, Status};
use tauri::{AppHandle, Manager, Runtime, Theme};

use crate::windows;

/// The system's light or dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// Which icon file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// macOS tints a template image itself.
    Template,
    /// Dark glyph for a light taskbar.
    Light,
    /// Light glyph for a dark taskbar.
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icon {
    pub variant: Variant,
    pub paused: bool,
}

impl Icon {
    /// macOS always uses the template image; other systems follow `appearance`.
    pub fn choose(os: Os, appearance: Appearance, paused: bool) -> Self {
        let variant = match (os, appearance) {
            (Os::MacOs, _) => Variant::Template,
            (_, Appearance::Light) => Variant::Light,
            (_, Appearance::Dark) => Variant::Dark,
        };
        Self { variant, paused }
    }

    pub fn is_template(self) -> bool {
        self.variant == Variant::Template
    }

    pub fn png(self) -> &'static [u8] {
        match (self.variant, self.paused) {
            (Variant::Template, false) => include_bytes!("../icons/tray-template-active.png"),
            (Variant::Template, true) => include_bytes!("../icons/tray-template-paused.png"),
            (Variant::Light, false) => include_bytes!("../icons/tray-light-active.png"),
            (Variant::Light, true) => include_bytes!("../icons/tray-light-paused.png"),
            (Variant::Dark, false) => include_bytes!("../icons/tray-dark-active.png"),
            (Variant::Dark, true) => include_bytes!("../icons/tray-dark-paused.png"),
        }
    }
}

pub const OPEN_ID: &str = "open";
pub const PAUSE_ID: &str = "pause";
pub const RESUME_ID: &str = "resume";
pub const ARRANGE_ID: &str = "arrange";
pub const QUIT_ID: &str = "quit";

/// One tray menu line; a line without an id is a heading that does nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: Option<&'static str>,
    pub label: String,
}

/// Everything the tray shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    pub icon: Icon,
    pub summary: String,
    pub entries: Vec<Entry>,
}

fn count(n: u32, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The tray for `status`.
pub fn view(status: &Status, os: Os, appearance: Appearance) -> TrayView {
    let summary = tray_summary(status);
    let paused = summary.state == EngineState::Paused;
    let words = if paused { "paused" } else { "active" };
    let summary = format!(
        "Mouser is {words}: {}, {}",
        count(summary.computers, "computer"),
        count(summary.displays, "display")
    );
    TrayView {
        icon: Icon::choose(os, appearance, paused),
        entries: vec![
            Entry {
                id: None,
                label: summary.clone(),
            },
            Entry {
                id: Some(OPEN_ID),
                label: "Open Mouser".into(),
            },
            if paused {
                Entry {
                    id: Some(RESUME_ID),
                    label: "Resume Mouser".into(),
                }
            } else {
                Entry {
                    id: Some(PAUSE_ID),
                    label: "Pause Mouser".into(),
                }
            },
            Entry {
                id: Some(ARRANGE_ID),
                label: "Arrange displays".into(),
            },
            Entry {
                id: Some(QUIT_ID),
                label: "Quit Mouser".into(),
            },
        ],
        summary,
    }
}

/// The live tray icon.
pub trait TrayPort: Send + Sync {
    fn apply(&self, view: &TrayView);
}

/// Keeps the tray current: the last status and the system appearance.
pub struct TrayController {
    port: Box<dyn TrayPort>,
    os: Os,
    last: Mutex<(Option<Status>, Appearance)>,
}

impl TrayController {
    pub fn new(port: Box<dyn TrayPort>, os: Os, appearance: Appearance) -> Self {
        Self {
            port,
            os,
            last: Mutex::new((None, appearance)),
        }
    }

    pub fn update_status(&self, status: Status) {
        let mut last = self.last.lock().unwrap();
        self.port.apply(&view(&status, self.os, last.1));
        last.0 = Some(status);
    }

    pub fn update_appearance(&self, appearance: Appearance) {
        let mut last = self.last.lock().unwrap();
        last.1 = appearance;
        let status = last
            .0
            .as_ref()
            .expect("a status arrived before any theme change");
        self.port.apply(&view(status, self.os, appearance));
    }
}

pub fn appearance_of(theme: Theme) -> Appearance {
    match theme {
        Theme::Dark => Appearance::Dark,
        _ => Appearance::Light,
    }
}

/// The system appearance as the main window sees it.
pub fn system_appearance<R: Runtime>(app: &AppHandle<R>) -> Appearance {
    let window = app.get_webview_window("main").expect("main window exists");
    appearance_of(window.theme().expect("theme is readable"))
}

/// What a tray menu line does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Pause,
    Resume,
    Arrange,
    Quit,
}

pub fn action(id: &str) -> Action {
    match id {
        OPEN_ID => Action::Open,
        PAUSE_ID => Action::Pause,
        RESUME_ID => Action::Resume,
        ARRANGE_ID => Action::Arrange,
        QUIT_ID => Action::Quit,
        other => panic!("the tray has no menu line \"{other}\""),
    }
}

/// Open the window the owner wants now, without blocking the menu.
pub fn open<R: Runtime>(app: &AppHandle<R>, control: &ControlHandle) {
    let (app, control) = (app.clone(), control.clone());
    tauri::async_runtime::spawn(async move { windows::open_home(&app, &control).await });
}

/// The `status` right now.
pub async fn current_status(control: &ControlHandle) -> Status {
    let value = control
        .call(Method::Status, None)
        .await
        .expect("status is available");
    serde_json::from_value(value).expect("status matches its schema")
}

/// Pause or resume crossing, as the tray line says.
pub async fn run_pause(control: &ControlHandle, action: Action) {
    let method = match action {
        Action::Pause => Method::ActivePause,
        Action::Resume => Method::ActiveResume,
        _ => panic!("only Pause and Resume change the engine state"),
    };
    control
        .call(method, None)
        .await
        .expect("the engine can be paused or resumed");
}
