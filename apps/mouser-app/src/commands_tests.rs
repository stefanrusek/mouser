use mouser_platform::PermissionKind as P;
use mouser_proto::control::FixAction;
use mouser_proto::model::PermissionKind;
use serde_json::json;
use tauri::Manager;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;

use crate::commands::{call, open_settings, run_fix};
use crate::support_tests::{NoSettingsScreen, app_with, fixture, fixture_on, has_window};

#[tokio::test(flavor = "multi_thread")]
async fn control_calls_the_method_with_that_name() {
    let f = fixture().await;
    let status = call(&f.host.control(), "status", None).await.unwrap();
    assert_eq!(status["computer"]["name"], "Desk Mac");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_method_is_an_error_naming_it() {
    let f = fixture().await;
    let err = call(&f.host.control(), "nope", None).await.unwrap_err();
    assert_eq!(err.code, -32601);
    assert_eq!(err.message, "Mouser does not know the request \"nope\".");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_permission_opens_its_own_system_screen() {
    let f = fixture().await;
    for kind in [
        PermissionKind::Accessibility,
        PermissionKind::InputMonitoring,
        PermissionKind::ScreenRecording,
        PermissionKind::FullDiskAccess,
        PermissionKind::InputCapturePortal,
        PermissionKind::RemoteDesktopPortal,
    ] {
        open_settings(&f.host, kind).unwrap();
    }
    assert_eq!(
        f.platform.opened_permission_settings(),
        vec![
            P::Accessibility,
            P::InputMonitoring,
            P::ScreenRecording,
            P::FullDiskAccess,
            P::InputCapturePortal,
            P::RemoteDesktopPortal,
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fixes_open_settings_or_the_join_prompt_and_the_rest_are_not_available() {
    let f = fixture().await;
    let platform = f.platform.clone();
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let host = handle.state::<crate::host::Host>();
    run_fix(
        &handle,
        &host,
        FixAction::OpenSystemSettings(PermissionKind::Accessibility),
    )
    .unwrap();
    assert_eq!(
        platform.opened_permission_settings(),
        vec![P::Accessibility]
    );
    assert!(!has_window(&handle, "join"));
    run_fix(&handle, &host, FixAction::ApproveJoin).unwrap();
    assert!(has_window(&handle, "join"));
    assert_eq!(
        run_fix(&handle, &host, FixAction::OpenUpdate),
        Err("The fix \"open_update\" is not available yet.".to_string())
    );
}

fn request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap(),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ui_reaches_all_four_commands_over_ipc() {
    let f = fixture().await;
    let platform = f.platform.clone();
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            crate::commands::control,
            crate::commands::open_permission_settings,
            crate::commands::fix,
            crate::commands::hide_window
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(f.host);
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let ipc = |cmd: &str, body: serde_json::Value| {
        get_ipc_response::<&tauri::WebviewWindow<MockRuntime>>(&&window, request(cmd, body))
            .map(|b| b.deserialize::<serde_json::Value>().unwrap())
    };

    let status = ipc("control", json!({"request": {"method": "status"}})).unwrap();
    assert_eq!(status["computer"]["name"], "Desk Mac");
    let missing = ipc("control", json!({"request": {}})).unwrap_err();
    assert!(missing.as_str().unwrap().contains("method"));
    let unknown = ipc("control", json!({"request": {"method": "nope"}})).unwrap_err();
    assert_eq!(unknown["code"], -32601);

    ipc(
        "open_permission_settings",
        json!({"kind": "input_monitoring"}),
    )
    .unwrap();
    ipc("fix", json!({"fix": "open_system_settings:accessibility"})).unwrap();
    assert_eq!(
        platform.opened_permission_settings(),
        vec![P::InputMonitoring, P::Accessibility]
    );
    let refused = ipc("fix", json!({"fix": "open_update"})).unwrap_err();
    assert_eq!(
        refused,
        json!("The fix \"open_update\" is not available yet.")
    );

    ipc("hide_window", json!({})).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_before_the_engine_is_up_fails_loudly() {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![crate::commands::control])
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let err = get_ipc_response::<&tauri::WebviewWindow<MockRuntime>>(
        &&window,
        request("control", json!({"request": {"method": "status"}})),
    )
    .unwrap_err();
    assert!(err.as_str().unwrap().contains("state not managed"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_settings_screen_that_cannot_open_says_why() {
    let virtual_platform = mouser_testkit::VirtualPlatform::new();
    let f = fixture_on(
        virtual_platform.clone(),
        std::sync::Arc::new(NoSettingsScreen(virtual_platform)),
    )
    .await;
    assert_eq!(
        open_settings(&f.host, PermissionKind::Accessibility),
        Err("There is no settings screen on this computer.".to_string())
    );
}
