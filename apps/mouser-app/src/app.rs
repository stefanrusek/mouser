//! The app's event pump: engine notifications drive the tray, the join
//! prompt, system notifications and the UI windows.

use std::sync::Arc;

use mouser_core::EngineEvent;

use crate::host::Control;
use mouser_proto::control::{Notification, Topic};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

use crate::host::Host;
use crate::hotkeys::Hotkeys;
use crate::notify::{self, Notifier};
use crate::overlay;
use crate::quickjump_windows;
use crate::tray::{TrayController, current_status};
use crate::windows;

/// The event every UI window listens to; the payload is the notification.
pub const EVENT: &str = "mouser://notification";

/// Act on one engine notification.
pub fn handle<R: Runtime>(
    app: &AppHandle<R>,
    tray: &TrayController,
    notifier: &dyn Notifier,
    note: Notification,
) {
    if note.method == Topic::Transfer {
        // Kept before it is announced, so a window that opens now still sees it.
        app.state::<Host>().remember_transfer(note.params.clone());
    }
    if note.method == Topic::Config {
        // Registered before the windows hear of it, so a window that asks which
        // hotkeys were refused sees the outcome of this change.
        app.state::<Arc<Hotkeys>>().on_config(&note.params);
    }
    app.emit(EVENT, &note).expect("UI windows can be notified");
    match note.method {
        Topic::Status => {
            let status: mouser_proto::control::Status =
                serde_json::from_value(note.params.clone()).expect("status matches its schema");
            app.state::<Host>().remember_names(&status);
            tray.update_status(status);
        }
        Topic::JoinRequest => windows::navigate(app, windows::DESK),
        Topic::Config | Topic::EngineEvent => {}
        Topic::Problem => app.state::<Host>().toast(&note, notifier),
        Topic::Transfer => {
            // Progress opens the window once; a question or a stop always brings it forward.
            let state = note.params["state"]
                .as_str()
                .expect("a transfer notification has a state");
            let asks = matches!(state, "awaiting_overwrite_decision" | "rejected" | "failed");
            if asks || (state != "completed" && !windows::is_shown(app)) {
                windows::navigate(app, windows::DESK);
            }
        }
    }
}

/// Handle notifications until the engine stops.
pub async fn pump<R: Runtime>(
    app: AppHandle<R>,
    tray: Arc<TrayController>,
    notifier: Arc<dyn Notifier>,
    mut notes: Receiver<Notification>,
) {
    loop {
        match notes.recv().await {
            Ok(note) => {
                let control = app.state::<Host>().control();
                handle(
                    &app,
                    &tray,
                    notify::gated(&control, notifier.as_ref()).await,
                    note,
                );
            }
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(_)) => {
                // Whatever was missed, the status says where things stand now.
                let control = app.state::<Host>().control();
                let status = current_status(&control).await;
                let note = Notification {
                    jsonrpc: "2.0".into(),
                    method: Topic::Status,
                    params: serde_json::to_value(status).expect("status serializes"),
                };
                handle(
                    &app,
                    &tray,
                    notify::gated(&control, notifier.as_ref()).await,
                    note,
                );
            }
        }
    }
}

/// Act on one engine event.
pub fn handle_event<R: Runtime>(app: &AppHandle<R>, notifier: &dyn Notifier, event: EngineEvent) {
    match event {
        EngineEvent::Landing { desktop, .. } => overlay::show_landing(app, desktop),
        EngineEvent::ReviewOffered { name, .. } => {
            notifier.show(
                &format!("Check where {name}'s screens sit"),
                &format!(
                    "{name} joined your desk. Open Mouser to review where its displays are placed."
                ),
            );
            windows::navigate(app, windows::DISPLAYS);
        }
        EngineEvent::QuickJumpShow { letters } => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let host = app.state::<Host>();
                let own = status_id(&host).await;
                let displays = host
                    .platform()
                    .displays()
                    .expect("the local displays can be listed");
                quickjump_windows::show(&app, &quickjump_windows::plan(&own, &displays, &letters));
            });
        }
        EngineEvent::QuickJumpHide => quickjump_windows::hide(app),
        // v2 placeholder: none (no Phase 9 task owns it; core logs the fault, nothing to show)
        EngineEvent::Fault { .. } => {}
        #[cfg(target_os = "windows")]
        EngineEvent::RestartElevated => {
            let app = app.clone();
            let elevation = app.state::<ElevationShell>();
            let (shell, exit) = (elevation.shell.clone(), elevation.exit.clone());
            tauri::async_runtime::spawn_blocking(move || {
                restart_elevated(&app, shell.as_ref(), &*exit);
            });
        }
        // Only a portable Windows computer is asked to restart elevated.
        #[cfg(not(target_os = "windows"))]
        EngineEvent::RestartElevated => {}
    }
}

/// What the restart as administrator asks of Windows, and how the app then
/// exits; tests put fakes here.
#[cfg(target_os = "windows")]
pub struct ElevationShell {
    pub shell: Arc<dyn crate::elevation::Shell + Send + Sync>,
    pub exit: Arc<dyn Fn() + Send + Sync>,
}

/// Restarts Mouser as administrator (FR-005): when the new instance is
/// starting, the engine is shut down and the app `exit`s; when the prompt
/// was declined, nothing changes.
#[cfg(target_os = "windows")]
pub fn restart_elevated<R: Runtime>(
    app: &AppHandle<R>,
    shell: &dyn crate::elevation::Shell,
    exit: impl FnOnce(),
) -> crate::elevation::State {
    let exe = std::env::current_exe().expect("the executable has a path");
    let log = app.state::<Host>().log();
    let state = crate::elevation::restart(shell, &exe, std::process::id(), &log);
    if state == crate::elevation::State::HandingOver {
        crate::exit::quit(app, exit);
    }
    state
}

/// This computer's id, from the engine's status.
async fn status_id(host: &Host) -> mouser_proto::ids::ComputerId {
    let status = host
        .control()
        .call(mouser_proto::control::Method::Status, None)
        .await
        .expect("status can be read");
    serde_json::from_value::<mouser_proto::control::Status>(status)
        .expect("status matches its schema")
        .computer
        .id
}

/// Handle engine events until the engine stops.
pub async fn pump_events<R: Runtime>(
    app: AppHandle<R>,
    notifier: Arc<dyn Notifier>,
    mut events: Receiver<EngineEvent>,
) {
    loop {
        match events.recv().await {
            Ok(event) => {
                let control = app.state::<Host>().control();
                handle_event(
                    &app,
                    notify::gated(&control, notifier.as_ref()).await,
                    event,
                );
            }
            Err(RecvError::Closed) => break,
            // Engine events are one-off moments; there is nothing to rebuild.
            Err(RecvError::Lagged(_)) => {}
        }
    }
}

/// Show the tray for the current status and open first-run if setup is
/// unfinished. Returns the engine's notifications from before the status was
/// read, for [`pump`].
pub async fn start<R: Runtime>(
    app: &AppHandle<R>,
    control: &Control,
    tray: &TrayController,
) -> Receiver<Notification> {
    mouser_core::faults::install(app.state::<Host>().log());
    let notes = control.notifications();
    let status = current_status(control).await;
    if windows::needs_setup(&status) {
        windows::navigate(app, windows::WELCOME);
    }
    app.state::<Host>().remember_names(&status);
    tray.update_status(status);
    notes
}
