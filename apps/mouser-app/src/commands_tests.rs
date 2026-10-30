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

use crate::commands::{FileManager, call, open_settings, reveal, run_fix};
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
            crate::commands::collect_logs,
            crate::commands::hotkey_refusals
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

    assert_eq!(ipc("hotkey_refusals", json!({})).unwrap(), json!({}));

    ipc("reveal_in_file_manager", json!({"path": "/tmp/bundle.zip"})).unwrap();
    assert_eq!(recorder.0.lock().unwrap().len(), 1);

    // `logs.collect {}` on this computer's engine: the bundle's path and one line per computer.
    let bundle = ipc("collect_logs", json!({})).unwrap();
    assert!(bundle["path"].as_str().unwrap().ends_with(".zip"));
    assert_eq!(bundle["computers"][0]["name"], "Desk Mac");
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

// ---- UI mode: logs.collect_stream, written by the user's own app (OD-7) ----

mod streamed_logs {
    use std::sync::Mutex;

    use base64::Engine as _;
    use mouser_core::paths::Env;
    use mouser_proto::control::{
        BundleComputer, LogsChunk, LogsCollectStreamResult, RpcError, Status,
    };
    use mouser_proto::wire::ComputerMode;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use crate::commands::{LogsLink, StreamedBundle, collect_logs_via};
    use crate::support_tests::{fixture, status_of};

    const NAME: &str = "mouser-logs-Desk-20261029T101500Z.zip";

    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn chunks_of(zip: &[u8], size: usize) -> Vec<LogsChunk> {
        zip.chunks(size)
            .enumerate()
            .map(|(seq, part)| LogsChunk {
                seq: seq as u32,
                data: base64::engine::general_purpose::STANDARD.encode(part),
            })
            .collect()
    }

    fn streamed(zip: &[u8]) -> StreamedBundle {
        StreamedBundle {
            result: LogsCollectStreamResult {
                name: NAME.into(),
                size: zip.len() as u64,
                sha256: hex(zip),
                computers: Vec::<BundleComputer>::new(),
            },
            chunks: chunks_of(zip, 4),
        }
    }

    struct Link {
        status: Status,
        stream: Mutex<Option<Result<StreamedBundle, RpcError>>>,
        calls: Mutex<Vec<&'static str>>,
    }

    impl LogsLink for Link {
        async fn status(&self) -> Result<Status, RpcError> {
            Ok(self.status.clone())
        }
        async fn collect(&self) -> Result<Value, RpcError> {
            self.calls.lock().unwrap().push("logs.collect");
            Ok(json!({"path": "/elsewhere.zip", "computers": []}))
        }
        async fn collect_stream(&self) -> Result<StreamedBundle, RpcError> {
            self.calls.lock().unwrap().push("logs.collect_stream");
            self.stream.lock().unwrap().take().unwrap()
        }
    }

    async fn link(mode: ComputerMode, stream: Result<StreamedBundle, RpcError>) -> Link {
        let f = fixture().await;
        let mut status = status_of(&f.host).await;
        let me = status.computer.id;
        status
            .computers
            .iter_mut()
            .find(|c| c.id == me)
            .unwrap()
            .mode = Some(mode);
        Link {
            status,
            stream: Mutex::new(Some(stream)),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn env(home: &std::path::Path) -> Env {
        Env {
            home: Some(home.to_path_buf()),
            appdata: None,
            xdg_config_home: None,
        }
    }

    fn files(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn in_service_mode_the_chunks_are_reassembled_and_written_to_downloads_with_the_collision_rule()
     {
        let home = tempfile::tempdir().unwrap();
        let zip = b"PK-the-whole-bundle".to_vec();
        let link = link(ComputerMode::Service, Ok(streamed(&zip))).await;
        let result = collect_logs_via(&link, &env(home.path()), home.path()).await.unwrap();
        let path = home.path().join("Downloads").join(NAME);
        assert_eq!(result["path"], path.to_string_lossy().as_ref());
        assert_eq!(result["computers"], json!([]));
        assert_eq!(std::fs::read(&path).unwrap(), zip);
        assert_eq!(*link.calls.lock().unwrap(), ["logs.collect_stream"]);

        let again = self::link(ComputerMode::Service, Ok(streamed(&zip))).await;
        let second = collect_logs_via(&again, &env(home.path()), home.path()).await.unwrap();
        let second_name = NAME.replace(".zip", " (2).zip");
        assert_eq!(
            second["path"],
            home.path().join("Downloads").join(&second_name).to_string_lossy().as_ref()
        );
        assert_eq!(files(&home.path().join("Downloads")), [second_name, NAME.to_string()]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_size_or_hash_mismatch_is_an_error_and_leaves_no_file() {
        let home = tempfile::tempdir().unwrap();
        let zip = b"PK-the-whole-bundle".to_vec();
        let mut short = streamed(&zip);
        short.chunks.pop();
        let mut flipped = streamed(&zip);
        flipped.result.sha256 = hex(b"something else");
        for bad in [short, flipped] {
            let link = link(ComputerMode::Service, Ok(bad)).await;
            let err = collect_logs_via(&link, &env(home.path()), home.path()).await.unwrap_err();
            assert_eq!(err.code, -32603);
            assert_eq!(
                err.message,
                "The log bundle from the Mouser service arrived damaged, so it was not saved."
            );
        }
        assert!(!home.path().join("Downloads").exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_engine_error_is_passed_on() {
        let home = tempfile::tempdir().unwrap();
        let refusal = RpcError { code: -32603, message: "no".into(), data: None };
        let link = link(ComputerMode::Service, Err(refusal.clone())).await;
        let err = collect_logs_via(&link, &env(home.path()), home.path()).await.unwrap_err();
        assert_eq!(err, refusal);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn outside_service_mode_the_engine_writes_the_file_with_logs_collect() {
        let home = tempfile::tempdir().unwrap();
        let link = link(ComputerMode::Installed, Ok(streamed(b"x"))).await;
        let result = collect_logs_via(&link, &env(home.path()), home.path()).await.unwrap();
        assert_eq!(result["path"], "/elsewhere.zip");
        assert_eq!(*link.calls.lock().unwrap(), ["logs.collect"]);
    }
}
