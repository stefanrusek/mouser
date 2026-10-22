//! The app's event pump: engine notifications drive the tray, the join
//! prompt, system notifications and the UI windows.

use std::sync::Arc;

use mouser_core::{ControlHandle, EngineEvent};
use mouser_proto::control::{Notification, Topic};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

use crate::notify::{self, Notifier};
use crate::overlay;
use crate::tray::{TrayController, current_status};
use crate::windows::{self, Which};

/// The event every UI window listens to; the payload is the notification.
pub const EVENT: &str = "mouser://notification";

/// Act on one engine notification.
pub fn handle<R: Runtime>(
    app: &AppHandle<R>,
    tray: &TrayController,
    notifier: &dyn Notifier,
    note: Notification,
) {
    app.emit(EVENT, &note).expect("UI windows can be notified");
    match note.method {
        Topic::Status => tray.update_status(
            serde_json::from_value(note.params.clone()).expect("status matches its schema"),
        ),
        Topic::JoinRequest => windows::show(app, Which::Join),
        Topic::Problem => notify::bridge(&note, notifier),
        Topic::Transfer => {
            // Progress opens the window once; a question always brings it forward.
            let state = note.params["state"]
                .as_str()
                .expect("a transfer notification has a state");
            let asks = state == "awaiting_overwrite_decision";
            if asks || (state != "completed" && !windows::is_open(app, Which::Transfer)) {
                windows::show(app, Which::Transfer);
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
            Ok(note) => handle(&app, &tray, notifier.as_ref(), note),
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(n)) => panic!("the app fell {n} notifications behind the engine"),
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
            windows::show(app, Which::Arranger);
        }
    }
}

/// Handle engine events until the engine stops.
pub async fn pump_events<R: Runtime>(
    app: AppHandle<R>,
    notifier: Arc<dyn Notifier>,
    mut events: Receiver<EngineEvent>,
) {
    loop {
        match events.recv().await {
            Ok(event) => handle_event(&app, notifier.as_ref(), event),
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(n)) => {
                panic!("the app fell {n} engine events behind the engine")
            }
        }
    }
}

/// Show the tray for the current status and open first-run if setup is
/// unfinished. Returns the engine's notifications from before the status was
/// read, for [`pump`].
pub async fn start<R: Runtime>(
    app: &AppHandle<R>,
    control: &ControlHandle,
    tray: &TrayController,
) -> Receiver<Notification> {
    let notes = control.notifications();
    let status = current_status(control).await;
    if windows::needs_setup(&status) {
        windows::show(app, Which::FirstRun);
    }
    tray.update_status(status);
    notes
}
