//! The app's event pump: engine notifications drive the tray, the join
//! prompt, system notifications and the UI windows.

use std::sync::Arc;

use mouser_core::ControlHandle;
use mouser_proto::control::{Notification, Topic};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;

use crate::notify::{self, Notifier};
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
        Topic::Transfer => {}
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
