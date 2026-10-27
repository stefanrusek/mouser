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

use crate::commands::{FileManager, call, call_and_sync, open_settings, reveal, run_fix};
use crate::hotkeys_tests::hotkeys_for;
use crate::support_tests::{
    NoSettingsScreen, app_with, fixture, fixture_on, has_window, status_of,
};

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

/// The id of the open problem about `kind`, once the engine has noticed it is missing.
async fn missing_permission_problem(f: &crate::support_tests::Fixture) -> String {
    f.platform.set_permission(
        mouser_platform::PermissionKind::Accessibility,
        mouser_platform::PermissionState::Missing,
    );
    for _ in 0..200 {
        let problems = status_of(&f.host).await.problems;
        if let Some(p) = problems
            .iter()
            .find(|p| p.fix == Some(FixAction::OpenSystemSettings(PermissionKind::Accessibility)))
        {
            return p.problem_id.clone();
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the engine never raised the missing permission");
}

#[tokio::test(flavor = "multi_thread")]
async fn fixes_run_through_the_engine_open_the_join_prompt_or_the_arranger_and_the_rest_are_not_available()
 {
    let f = fixture().await;
    let id = missing_permission_problem(&f).await;
    let platform = f.platform.clone();
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let host = handle.state::<crate::host::Host>();
    run_fix(
        &handle,
        &host,
        &id,
        FixAction::OpenSystemSettings(PermissionKind::Accessibility),
    )
    .unwrap();
    assert_eq!(
        platform.opened_permission_settings(),
        vec![P::Accessibility]
    );
    assert!(
        run_fix(
            &handle,
            &host,
            "nope",
            FixAction::OpenSystemSettings(PermissionKind::Accessibility)
        )
        .unwrap_err()
        .contains("nope is not an open problem")
    );
    assert!(!has_window(&handle, "main"));
    run_fix(&handle, &host, "x", FixAction::ApproveJoin).unwrap();
    assert!(has_window(&handle, "main"));
    assert_eq!(
        run_fix(&handle, &host, "x", FixAction::OpenUpdate),
        Err("The fix \"open_update\" is not available yet.".to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_edge_fix_opens_the_arranger_on_the_edge_the_problem_names_and_asks_the_engine_to_clear_it()
 {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let host = handle.state::<crate::host::Host>();
    let id = "crossing_blocked:aa/DP-1/right";
    // No such notice is open here, so the engine says so; the arranger still goes to the edge.
    assert_eq!(
        run_fix(&handle, &host, id, FixAction::OpenArrangerEdge),
        Err(format!("{id} is not an open problem"))
    );
    assert!(has_window(&handle, "main"));
    assert_eq!(host.take_edge(), Some(id.to_string()));
}

type Opened = Vec<(String, Vec<String>)>;

#[derive(Default, Clone)]
struct Recorder(std::sync::Arc<std::sync::Mutex<Opened>>);

impl FileManager for Recorder {
    fn open(&self, program: &str, args: &[String]) {
        self.0
            .lock()
            .unwrap()
            .push((program.to_string(), args.to_vec()));
    }
}

#[test]
fn revealing_a_path_opens_each_systems_file_manager_at_it() {
    for (os, path, program, args) in [
        (
            mouser_platform::Os::MacOs,
            "/Users/me/Downloads/bundle.zip",
            "open",
            vec!["-R", "/Users/me/Downloads/bundle.zip"],
        ),
        (
            mouser_platform::Os::Windows,
            r"C:\Users\me\Downloads\bundle.zip",
            "explorer",
            vec![r"/select,C:\Users\me\Downloads\bundle.zip"],
        ),
        (
            mouser_platform::Os::Linux,
            "/home/me/Downloads/bundle.zip",
            "xdg-open",
            vec!["/home/me/Downloads"],
        ),
    ] {
        let recorder = Recorder::default();
        reveal(os, path, &recorder);
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![(
                program.to_string(),
                args.into_iter().map(String::from).collect()
            )]
        );
    }
}

pub(crate) fn request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
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
async fn the_ui_reaches_all_the_commands_over_ipc() {
    let f = fixture().await;
    let id = missing_permission_problem(&f).await;
    let platform = f.platform.clone();
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            crate::commands::control,
            crate::commands::open_permission_settings,
            crate::commands::fix,
            crate::commands::transfers,
            crate::commands::take_arranger_edge,
            crate::commands::transfer_resolve,
            crate::commands::hide_window,
            crate::commands::reveal_in_file_manager,
            crate::commands::collect_logs
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    app.manage(hotkeys_for(f.host.control()).hotkeys);
    app.manage(f.host);
    let recorder = Recorder::default();
    app.manage::<Box<dyn FileManager>>(Box::new(recorder.clone()));
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
    ipc(
        "fix",
        json!({"problemId": id, "fix": "open_system_settings:accessibility"}),
    )
    .unwrap();
    assert_eq!(
        platform.opened_permission_settings(),
        vec![P::InputMonitoring, P::Accessibility]
    );
    let refused = ipc("fix", json!({"problemId": "x", "fix": "open_update"})).unwrap_err();
    assert_eq!(
        refused,
        json!("The fix \"open_update\" is not available yet.")
    );

    assert_eq!(ipc("transfers", json!({})).unwrap(), json!([]));
    assert_eq!(ipc("take_arranger_edge", json!({})).unwrap(), json!(null));

    // No transfer is waiting, so an answer is quietly dropped.
    ipc(
        "transfer_resolve",
        json!({"transferId": "ab12", "overwrite": true}),
    )
    .unwrap();

    ipc("hide_window", json!({})).unwrap();

    ipc("reveal_in_file_manager", json!({"path": "/tmp/bundle.zip"})).unwrap();
    assert_eq!(recorder.0.lock().unwrap().len(), 1);

    // `logs.collect {}` on this computer's engine: the bundle's path and one line per computer.
    let bundle = ipc("collect_logs", json!({})).unwrap();
    assert!(bundle["path"].as_str().unwrap().ends_with(".zip"));
    assert_eq!(bundle["computers"][0]["name"], "Desk Mac");
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_a_setting_picks_up_changed_hotkeys() {
    let f = fixture().await;
    let set = hotkeys_for(f.host.control());
    set.hotkeys.sync();
    set.config.lock().unwrap().hotkeys.quickjump = "Ctrl+Alt+K".into();
    let control = f.host.control();

    call_and_sync(&control, &set.hotkeys, "status", None)
        .await
        .unwrap();
    assert_eq!(set.shortcuts.replaced.lock().unwrap().len(), 1);

    // Whether or not the engine accepts the setting, the hotkeys follow config.
    let _ = call_and_sync(&control, &set.hotkeys, "config.set", None).await;
    assert_eq!(set.shortcuts.replaced.lock().unwrap().len(), 2);
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

#[tokio::test(flavor = "multi_thread")]
async fn collecting_logs_before_the_engine_is_up_fails_loudly() {
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![crate::commands::collect_logs])
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let err = get_ipc_response::<&tauri::WebviewWindow<MockRuntime>>(
        &&window,
        request("collect_logs", json!({})),
    )
    .unwrap_err();
    assert!(err.as_str().unwrap().contains("state not managed"));
}
