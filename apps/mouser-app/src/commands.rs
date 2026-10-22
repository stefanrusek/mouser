//! The commands the UI windows call. Everything else the UI needs goes
//! through `control`, which is the control API, method for method.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouser_core::ControlHandle;
use mouser_platform::{Os, PermissionKind as PlatformKind};
use mouser_proto::control::{FixAction, Method, RpcError};
use mouser_proto::model::PermissionKind;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime, State, WebviewWindow};

use crate::host::Host;
use crate::hotkeys::Hotkeys;
use crate::windows::{self, Which};

fn platform_kind(kind: PermissionKind) -> PlatformKind {
    match kind {
        PermissionKind::Accessibility => PlatformKind::Accessibility,
        PermissionKind::InputMonitoring => PlatformKind::InputMonitoring,
        PermissionKind::ScreenRecording => PlatformKind::ScreenRecording,
        PermissionKind::FullDiskAccess => PlatformKind::FullDiskAccess,
        PermissionKind::InputCapturePortal => PlatformKind::InputCapturePortal,
        PermissionKind::RemoteDesktopPortal => PlatformKind::RemoteDesktopPortal,
    }
}

/// Call a control-API method by its JSON-RPC name.
pub async fn call(
    control: &ControlHandle,
    method: &str,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    match Method::from_name(method) {
        Some(method) => control.call(method, params).await,
        None => Err(RpcError {
            code: -32601,
            message: format!("Mouser does not know the request \"{method}\"."),
            data: None,
        }),
    }
}

/// [`call`], then pick up any hotkey the call may have changed.
pub async fn call_and_sync(
    control: &ControlHandle,
    hotkeys: &Arc<Hotkeys>,
    method: &str,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let result = call(control, method, params).await;
    if method == "config.set" {
        hotkeys.sync();
    }
    result
}

/// Open the operating system's screen for granting `kind`.
pub fn open_settings(host: &Host, kind: PermissionKind) -> Result<(), String> {
    host.platform()
        .open_permission_settings(platform_kind(kind))
        .map_err(|e| e.to_string())
}

/// Do what the open problem `problem_id`'s one-click fix says; the engine
/// acknowledges it (opening the settings screen, clearing the notice).
pub fn run_fix<R: Runtime>(
    app: &AppHandle<R>,
    host: &Host,
    problem_id: &str,
    fix: FixAction,
) -> Result<(), String> {
    match fix {
        FixAction::OpenSystemSettings(_) => host.control().fix(problem_id),
        FixAction::ApproveJoin => {
            windows::show(app, Which::Join);
            Ok(())
        }
        FixAction::OpenArrangerEdge => {
            // The id names the blocked edge; the arranger opens that one.
            host.ask_for_edge(problem_id.to_string());
            windows::show(app, Which::Arranger);
            host.control().fix(problem_id)
        }
        other => Err(format!("The fix \"{other}\" is not available yet.")),
    }
}

/// What the UI sends to `control`.
#[derive(Deserialize)]
pub struct ControlRequest {
    method: String,
    params: Option<Value>,
}

#[tauri::command]
pub async fn control(
    host: State<'_, Host>,
    hotkeys: State<'_, Arc<Hotkeys>>,
    request: ControlRequest,
) -> Result<Value, RpcError> {
    call_and_sync(&host.control(), &hotkeys, &request.method, request.params).await
}

#[tauri::command]
pub fn open_permission_settings(host: State<'_, Host>, kind: PermissionKind) -> Result<(), String> {
    open_settings(&host, kind)
}

#[tauri::command]
pub fn fix<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, Host>,
    problem_id: String,
    fix: FixAction,
) -> Result<(), String> {
    run_fix(&app, &host, &problem_id, fix)
}

/// The latest `transfer` notification of every transfer so far, for a window
/// that opened after them.
#[tauri::command]
pub fn transfers(host: State<'_, Host>) -> Vec<Value> {
    host.transfers()
}

/// The blocked-edge problem id the arranger was asked to open, once.
#[tauri::command]
pub fn take_arranger_edge(host: State<'_, Host>) -> Option<String> {
    host.take_edge()
}

/// The program that shows a folder in the system file manager, and the folder
/// that holds `file` (the log file's folder).
pub fn folder_opener(os: Os, file: &str) -> (&'static str, PathBuf) {
    let program = match os {
        Os::MacOs => "open",
        Os::Windows => "explorer",
        Os::Linux => "xdg-open",
    };
    let folder = Path::new(file)
        .parent()
        .expect("a log file is inside a folder");
    (program, folder.to_path_buf())
}

/// Answer an `awaiting_overwrite_decision` question of a dragged-file transfer.
#[tauri::command]
pub fn transfer_resolve(host: State<'_, Host>, transfer_id: String, overwrite: bool) {
    host.control().resolve_transfer(&transfer_id, overwrite);
}

/// Hide the calling window (it is created again when needed).
#[tauri::command]
pub fn hide_window<R: Runtime>(window: WebviewWindow<R>) {
    window.hide().expect("window can be hidden");
}
