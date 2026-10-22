use std::path::Path;
use std::sync::Arc;

use mouser_core::paths::{Env, PORTABLE_DIR, Paths};
use mouser_testkit::VirtualPlatform;
use tauri::Manager;
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets};

use crate::host::Host;
use crate::portable::{create_platform, engine_params, webview_data_dir};
use crate::support_tests::{app_with, control_path_in, eventually, fixture, free_port, has_window};
use crate::windows::{Which, show};

fn drive() -> (tempfile::TempDir, Paths) {
    let drive = tempfile::tempdir().unwrap();
    std::fs::create_dir(drive.path().join(PORTABLE_DIR)).unwrap();
    let exe = drive.path().join("Mouser");
    let env = Env {
        home: Some("/nowhere".into()),
        appdata: Some("/nowhere".into()),
        xdg_config_home: None,
    };
    let paths = Paths::resolve(&exe, crate::platform_select::current_os(), &env);
    assert!(paths.portable);
    (drive, paths)
}

fn installed() -> Paths {
    Paths {
        dir: "/home/me/.config/mouser".into(),
        portable: false,
    }
}

#[test]
fn the_webview_keeps_its_data_in_mouser_data_only_when_portable() {
    let (drive, paths) = drive();
    assert_eq!(
        webview_data_dir(&paths),
        Some(drive.path().join(PORTABLE_DIR).join("webview"))
    );
    assert_eq!(webview_data_dir(&installed()), None);
}

#[test]
fn engine_parameters_follow_the_paths() {
    let (drive, paths) = drive();
    let params = engine_params(
        Arc::new(VirtualPlatform::new()),
        &paths,
        Path::new("control").into(),
        "Desk".into(),
        "1".into(),
    );
    assert_eq!(params.config_dir, drive.path().join(PORTABLE_DIR));
    assert!(params.portable);
    assert!(params.secrets.is_some());
    let params = engine_params(
        Arc::new(VirtualPlatform::new()),
        &installed(),
        Path::new("control").into(),
        "Desk".into(),
        "1".into(),
    );
    assert!(!params.portable);
    assert!(params.secrets.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_portable_host_writes_under_mouser_data_and_never_sets_autostart() {
    let (drive, paths) = drive();
    let control_dir = tempfile::tempdir().unwrap();
    let platform = VirtualPlatform::new();
    let mut params = engine_params(
        Arc::new(platform.clone()),
        &paths,
        control_path_in(&control_dir),
        "Desk Mac".into(),
        "1.2.3".into(),
    );
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    mouser_core::config::save(&paths.dir, &config);
    params.installer = None;
    let host = Host::start_with(params).await;
    host.control()
        .call(
            mouser_proto::control::Method::ConfigSet,
            Some(serde_json::json!({"key": "general.start_at_login", "value": true})),
        )
        .await
        .unwrap();
    assert!(!platform.is_autostart_enabled());
    assert!(drive.path().join(PORTABLE_DIR).join("config.toml").exists());
    let entries: Vec<_> = std::fs::read_dir(drive.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from(PORTABLE_DIR)]);
}

#[cfg(target_os = "macos")]
#[test]
fn a_portable_mac_says_permissions_are_needed_on_each_computer() {
    let (_drive, paths) = drive();
    let limits = create_platform(&paths).info().limitations;
    assert!(limits.iter().any(|l| l.text.contains("granted again")));
    let limits = create_platform(&installed()).info().limitations;
    assert!(limits.iter().all(|l| !l.text.contains("granted again")));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_creates_the_linux_platform_for_either_mode() {
    let (_drive, paths) = drive();
    assert_eq!(
        create_platform(&paths).info().os,
        mouser_platform::Os::Linux
    );
    assert_eq!(
        create_platform(&installed()).info().os,
        mouser_platform::Os::Linux
    );
}

#[cfg(target_os = "windows")]
#[test]
#[should_panic(expected = "platform backend is not implemented yet")]
fn windows_fails_loudly_until_its_backend_exists() {
    create_platform(&installed());
}

#[tokio::test(flavor = "multi_thread")]
async fn windows_open_with_or_without_portable_state() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    show(&handle, Which::Health);
    assert!(has_window(&handle, "health"));
    let (_drive, paths) = drive();
    app.manage(paths);
    show(&handle, Which::Transfer);
    eventually(|| has_window(&handle, "transfer")).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ui_can_ask_whether_this_run_is_portable() {
    for (paths, expected) in [(drive().1, true), (installed(), false)] {
        let app = mock_builder()
            .invoke_handler(tauri::generate_handler![crate::portable::is_portable])
            .build(mock_context(noop_assets()))
            .unwrap();
        app.manage(paths);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let answer = get_ipc_response::<&tauri::WebviewWindow<tauri::test::MockRuntime>>(
            &&window,
            crate::commands_tests::request("is_portable", serde_json::json!({})),
        )
        .unwrap();
        let answer: bool = match answer {
            tauri::ipc::InvokeResponseBody::Json(json) => serde_json::from_str(&json).unwrap(),
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(answer, expected);
    }
}
