//! The commands the UI windows call. Everything else the UI needs goes
//! through `control`, which is the control API, method for method.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use mouser_core::ControlHandle;
use mouser_core::paths::Env;
use mouser_platform::{Os, PermissionKind as PlatformKind};
use mouser_proto::control::{
    FixAction, LogsChunk, LogsCollectStreamResult, Method, RpcError, Status,
};
use mouser_proto::model::PermissionKind;
use mouser_proto::wire::ComputerMode;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Runtime, State, WebviewWindow};

use crate::host::Host;
use crate::hotkeys::Hotkeys;
use crate::windows;

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
            windows::navigate(app, windows::DESK);
            Ok(())
        }
        FixAction::OpenArrangerEdge => {
            // The id names the blocked edge; the arranger opens that one.
            host.ask_for_edge(problem_id.to_string());
            windows::navigate(app, windows::DISPLAYS);
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
pub async fn control(host: State<'_, Host>, request: ControlRequest) -> Result<Value, RpcError> {
    call(&host.control(), &request.method, request.params).await
}

/// The hotkeys the operating system refused to register, by name, with its reason.
#[tauri::command]
pub fn hotkey_refusals(hotkeys: State<'_, Arc<Hotkeys>>) -> BTreeMap<String, String> {
    hotkeys.refusals()
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

/// Starts the system file manager (the live one spawns a process).
pub trait FileManager: Send + Sync {
    fn open(&self, program: &str, args: &[String]);
}

/// Show `path` in the system file manager: Finder and Explorer select the
/// file, `xdg-open` opens the folder that holds it.
pub fn reveal(os: Os, path: &str, manager: &dyn FileManager) {
    match os {
        Os::MacOs => manager.open("open", &["-R".to_string(), path.to_string()]),
        Os::Windows => manager.open("explorer", &[format!("/select,{path}")]),
        Os::Linux => {
            let folder = Path::new(path).parent().expect("a file is inside a folder");
            manager.open("xdg-open", &[folder.display().to_string()]);
        }
    }
}

/// Show `path` (a log file or a bundle) in the system file manager.
#[tauri::command]
pub fn reveal_in_file_manager(manager: State<'_, Box<dyn FileManager>>, path: String) {
    reveal(
        crate::platform_select::current_os(),
        &path,
        manager.as_ref(),
    );
}

/// Collect every desk computer's logs into one bundle on this computer:
/// `logs.collect {}` on this app's engine, as `{path, computers}`.
#[tauri::command]
pub async fn collect_logs(host: State<'_, Host>) -> Result<Value, RpcError> {
    host.control()
        .call(Method::LogsCollect, Some(serde_json::json!({})))
        .await
}

/// A bundle as `logs.collect_stream` delivers it: its description, and the
/// `logs.chunk` notifications that came before it.
pub struct StreamedBundle {
    pub result: LogsCollectStreamResult,
    pub chunks: Vec<LogsChunk>,
}

/// The engine this app talks to for `collect_logs`: in UI mode, the session
/// engine behind the service pipe, which can stream chunks to this caller.
pub trait LogsLink {
    fn status(&self) -> impl Future<Output = Result<Status, RpcError>> + Send;
    fn collect(&self) -> impl Future<Output = Result<Value, RpcError>> + Send;
    fn collect_stream(&self) -> impl Future<Output = Result<StreamedBundle, RpcError>> + Send;
}

/// Collect the logs the way this computer's mode says (OD-7): the installed
/// service (`service`) streams the bundle and this app writes it, as the
/// user, to their Downloads folder; any other engine writes it itself.
pub async fn collect_logs_via(
    link: &impl LogsLink,
    env: &Env,
    machine_dir: &Path,
) -> Result<Value, RpcError> {
    let status = link.status().await?;
    let me = status.computer.id;
    let mode = status.computers.iter().find(|c| c.id == me).and_then(|c| c.mode);
    if mode != Some(ComputerMode::Service) {
        return link.collect().await;
    }
    let streamed = link.collect_stream().await?;
    let dir = mouser_core::logs::bundle::location(None, ComputerMode::Service, machine_dir, env);
    let saved = crate::bundle_save::save(streamed.result, &streamed.chunks, &dir)?;
    Ok(serde_json::to_value(saved).expect("result serializes"))
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
