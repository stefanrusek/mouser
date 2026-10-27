//! The menu-bar / system-tray model: what the icon, tooltip and menu say,
//! computed from `status`. The live tray is driven through [`TrayPort`].

use std::sync::Mutex;

use mouser_core::ControlHandle;
use mouser_platform::Os;
use mouser_proto::control::{
    ConnectionState, EngineState, Method, Status, TrayConnection, UpdateResult,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime, Theme};

use crate::{tray_sas, updater, windows};

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
    pub connection: TrayConnection,
}

impl Icon {
    /// macOS always uses the template image; other systems follow `appearance`.
    pub fn choose(
        os: Os,
        appearance: Appearance,
        paused: bool,
        connection: TrayConnection,
    ) -> Self {
        let variant = match (os, appearance) {
            (Os::MacOs, _) => Variant::Template,
            (_, Appearance::Light) => Variant::Light,
            (_, Appearance::Dark) => Variant::Dark,
        };
        Self {
            variant,
            paused,
            connection,
        }
    }

    pub fn is_template(self) -> bool {
        self.variant == Variant::Template
    }

    /// The PNG `icons/tray-<theme>-<state>-<connection>.png`.
    pub fn png(self) -> &'static [u8] {
        use TrayConnection::{Alone, Connected, Disconnected};
        match (self.variant, self.paused, self.connection) {
            (Variant::Template, false, Connected) => {
                include_bytes!("../icons/tray-template-active-connected.png")
            }
            (Variant::Template, false, Disconnected) => {
                include_bytes!("../icons/tray-template-active-disconnected.png")
            }
            (Variant::Template, false, Alone) => {
                include_bytes!("../icons/tray-template-active-alone.png")
            }
            (Variant::Template, true, Connected) => {
                include_bytes!("../icons/tray-template-paused-connected.png")
            }
            (Variant::Template, true, Disconnected) => {
                include_bytes!("../icons/tray-template-paused-disconnected.png")
            }
            (Variant::Template, true, Alone) => {
                include_bytes!("../icons/tray-template-paused-alone.png")
            }
            (Variant::Light, false, Connected) => {
                include_bytes!("../icons/tray-light-active-connected.png")
            }
            (Variant::Light, false, Disconnected) => {
                include_bytes!("../icons/tray-light-active-disconnected.png")
            }
            (Variant::Light, false, Alone) => {
                include_bytes!("../icons/tray-light-active-alone.png")
            }
            (Variant::Light, true, Connected) => {
                include_bytes!("../icons/tray-light-paused-connected.png")
            }
            (Variant::Light, true, Disconnected) => {
                include_bytes!("../icons/tray-light-paused-disconnected.png")
            }
            (Variant::Light, true, Alone) => include_bytes!("../icons/tray-light-paused-alone.png"),
            (Variant::Dark, false, Connected) => {
                include_bytes!("../icons/tray-dark-active-connected.png")
            }
            (Variant::Dark, false, Disconnected) => {
                include_bytes!("../icons/tray-dark-active-disconnected.png")
            }
            (Variant::Dark, false, Alone) => include_bytes!("../icons/tray-dark-active-alone.png"),
            (Variant::Dark, true, Connected) => {
                include_bytes!("../icons/tray-dark-paused-connected.png")
            }
            (Variant::Dark, true, Disconnected) => {
                include_bytes!("../icons/tray-dark-paused-disconnected.png")
            }
            (Variant::Dark, true, Alone) => include_bytes!("../icons/tray-dark-paused-alone.png"),
        }
    }
}

pub const OPEN_ID: &str = "open";
pub const PAUSE_ID: &str = "pause";
pub const RESUME_ID: &str = "resume";
pub const ARRANGE_ID: &str = "arrange";
pub const HEALTH_ID: &str = "health";
pub const UPDATE_ID: &str = "update";
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
    /// The hover text: names the active or paused state and the connection.
    pub tooltip: String,
    pub entries: Vec<Entry>,
}

fn count(n: u32, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The tray for `status`, with the version line when `update.check` has answered.
pub fn view(
    status: &Status,
    os: Os,
    appearance: Appearance,
    update: Option<&UpdateResult>,
) -> TrayView {
    let summary = status.tray_summary;
    let paused = summary.state == EngineState::Paused;
    let summary_connection = summary.connection;
    let words = if paused { "paused" } else { "active" };
    let summary = format!(
        "Mouser is {words}: {}, {}",
        count(summary.computers, "computer"),
        count(summary.displays, "display")
    );
    let state_words = if paused { "paused" } else { "active" };
    let tooltip = match summary_connection {
        TrayConnection::Connected => format!("Mouser is {state_words} · connected"),
        TrayConnection::Alone => {
            format!("Mouser is {state_words} · no other computers on this desk")
        }
        TrayConnection::Disconnected => {
            let names: Vec<&str> = status
                .computers
                .iter()
                .filter(|c| {
                    !matches!(
                        c.connection,
                        ConnectionState::SelfComputer | ConnectionState::Connected
                    )
                })
                .map(|c| c.name.as_str())
                .collect();
            format!(
                "Mouser is {state_words} · not connected to {}",
                names.join(", ")
            )
        }
    };
    let version = update.map(|update| {
        let (id, label) = updater::line(update);
        Entry { id, label }
    });
    let mut entries = vec![
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
            id: Some(HEALTH_ID),
            label: "Mouser health".into(),
        },
        Entry {
            id: Some(QUIT_ID),
            label: "Quit Mouser".into(),
        },
    ];
    entries.splice(1..1, version);
    let quit = entries.len() - 1;
    entries.splice(quit..quit, tray_sas::entry(status));
    TrayView {
        icon: Icon::choose(os, appearance, paused, summary_connection),
        entries,
        summary,
        tooltip,
    }
}

/// The live tray icon.
pub trait TrayPort: Send + Sync {
    fn apply(&self, view: &TrayView);
}

struct Last {
    status: Option<Status>,
    appearance: Appearance,
    update: Option<UpdateResult>,
}

/// Keeps the tray current: the last status and the system appearance.
pub struct TrayController {
    port: Box<dyn TrayPort>,
    os: Os,
    last: Mutex<Last>,
}

impl TrayController {
    pub fn new(port: Box<dyn TrayPort>, os: Os, appearance: Appearance) -> Self {
        Self {
            port,
            os,
            last: Mutex::new(Last {
                status: None,
                appearance,
                update: None,
            }),
        }
    }

    pub fn update_status(&self, status: Status) {
        let mut last = self.last.lock().unwrap();
        self.port.apply(&view(
            &status,
            self.os,
            last.appearance,
            last.update.as_ref(),
        ));
        last.status = Some(status);
    }

    pub fn update_appearance(&self, appearance: Appearance) {
        let mut last = self.last.lock().unwrap();
        last.appearance = appearance;
        self.redraw(&last);
    }

    /// What `update.check` found. The tray shows it with the next status if
    /// none has arrived yet.
    pub fn update_update(&self, update: UpdateResult) {
        let mut last = self.last.lock().unwrap();
        last.update = Some(update);
        if last.status.is_some() {
            self.redraw(&last);
        }
    }

    fn redraw(&self, last: &Last) {
        let status = last
            .status
            .as_ref()
            .expect("a status arrived before any theme change");
        self.port.apply(&view(
            status,
            self.os,
            last.appearance,
            last.update.as_ref(),
        ));
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
    Health,
    Update,
    SendCad,
    Quit,
}

/// The action of menu line `id`; a line this tray does not have does nothing.
pub fn action(id: &str) -> Option<Action> {
    Some(match id {
        OPEN_ID => Action::Open,
        PAUSE_ID => Action::Pause,
        RESUME_ID => Action::Resume,
        ARRANGE_ID => Action::Arrange,
        HEALTH_ID => Action::Health,
        UPDATE_ID => Action::Update,
        tray_sas::SEND_CAD_ID => Action::SendCad,
        QUIT_ID => Action::Quit,
        _ => return None,
    })
}

/// Do what menu line `id` says. Installing an update, sending Ctrl-Alt-Del and
/// quitting need the live shell, so they are handed to `rest`.
pub fn handle_menu<R: Runtime>(
    app: &AppHandle<R>,
    control: &ControlHandle,
    id: &str,
    rest: &mut dyn FnMut(Action),
) {
    match action(id) {
        None => {}
        Some(Action::Open) => open(app, control),
        Some(Action::Arrange) => windows::navigate(app, windows::DISPLAYS),
        Some(Action::Health) => windows::navigate(app, windows::HEALTH),
        Some(action @ (Action::Pause | Action::Resume)) => {
            let control = control.clone();
            tauri::async_runtime::spawn(async move { run_pause(&control, action).await });
        }
        Some(action @ (Action::Update | Action::SendCad | Action::Quit)) => rest(action),
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

/// Pause if Mouser is active and resume if it is paused.
pub async fn toggle_pause(control: &ControlHandle) {
    let action = match current_status(control).await.state {
        EngineState::Active => Action::Pause,
        EngineState::Paused => Action::Resume,
    };
    run_pause(control, action).await;
}

/// A click on the icon: the left button released toggles pause once per
/// click (the menu opens on the right button, not the left).
pub fn handle_tray_event(control: &ControlHandle, event: &TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        let control = control.clone();
        tauri::async_runtime::spawn(async move { toggle_pause(&control).await });
    }
}
