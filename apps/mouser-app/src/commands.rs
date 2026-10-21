//! The commands the UI windows call. Everything else the UI needs goes
//! through `control`, which is the control API, method for method.

use mouser_core::ControlHandle;
use mouser_platform::PermissionKind as PlatformKind;
use mouser_proto::control::{FixAction, Method, RpcError};
use mouser_proto::model::PermissionKind;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime, State, WebviewWindow};

use crate::host::Host;
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

/// Open the operating system's screen for granting `kind`.
pub fn open_settings(host: &Host, kind: PermissionKind) -> Result<(), String> {
    host.platform()
        .open_permission_settings(platform_kind(kind))
        .map_err(|e| e.to_string())
}

/// Do what a problem's one-click fix says.
pub fn run_fix<R: Runtime>(app: &AppHandle<R>, host: &Host, fix: FixAction) -> Result<(), String> {
    match fix {
        FixAction::OpenSystemSettings(kind) => open_settings(host, kind),
        FixAction::ApproveJoin => {
            windows::show(app, Which::Join);
            Ok(())
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
pub async fn control(host: State<'_, Host>, request: ControlRequest) -> Result<Value, RpcError> {
    call(&host.control(), &request.method, request.params).await
}

#[tauri::command]
pub fn open_permission_settings(host: State<'_, Host>, kind: PermissionKind) -> Result<(), String> {
    open_settings(&host, kind)
}

#[tauri::command]
pub fn fix<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, Host>,
    fix: FixAction,
) -> Result<(), String> {
    run_fix(&app, &host, fix)
}

/// Hide the calling window (it is created again when needed).
#[tauri::command]
pub fn hide_window<R: Runtime>(window: WebviewWindow<R>) {
    window.hide().expect("window can be hidden");
}
