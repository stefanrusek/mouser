use std::cell::Cell;

use crate::exit::{on_exit, quit};
use crate::support_tests::{app_with, control_path_in, fixture};

#[test]
fn the_exit_event_shuts_the_host_down() {
    let (app, path, _dir) = tauri::async_runtime::block_on(async {
        let f = fixture().await;
        let path = control_path_in(&f._dir);
        (app_with(f.host), path, f._dir)
    });
    assert!(path.exists());
    on_exit(app.handle());
    assert!(!path.exists());
}

#[cfg(target_os = "windows")]
#[test]
fn exiting_removes_the_elevated_mark() {
    use tauri::Manager;
    let (app, _dir) = tauri::async_runtime::block_on(async {
        let f = fixture().await;
        (app_with(f.host), f._dir)
    });
    let data = tempfile::tempdir().unwrap();
    let paths = mouser_core::paths::Paths {
        dir: data.path().to_path_buf(),
        portable: true,
    };
    let marker = crate::elevation::Marker::write(&paths, true, std::process::id());
    app.manage(crate::exit::Elevated(std::sync::Mutex::new(marker)));
    let file = data.path().join(crate::elevation::MARKER);
    assert!(file.exists());
    on_exit(app.handle());
    assert!(!file.exists());
}

#[test]
fn tray_quit_shuts_the_host_down_before_the_app_exits() {
    let (app, path, _dir) = tauri::async_runtime::block_on(async {
        let f = fixture().await;
        let path = control_path_in(&f._dir);
        (app_with(f.host), path, f._dir)
    });
    let exited_after_shutdown = Cell::new(None);
    quit(app.handle(), || {
        exited_after_shutdown.set(Some(!path.exists()));
    });
    assert_eq!(exited_after_shutdown.get(), Some(true));
}

#[test]
fn tray_quit_in_ui_mode_asks_the_session_engine_to_quit_before_the_app_exits() {
    let (f, mut events, app) = tauri::async_runtime::block_on(async {
        let f = fixture().await;
        let events = f.host.control().events();
        let ui = crate::host_tests::remote::ui_host(&f).await;
        (f, events, app_with(ui))
    });
    let exited = Cell::new(false);
    quit(app.handle(), || exited.set(true));
    assert!(exited.get());
    tauri::async_runtime::block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while events.recv().await.unwrap() != mouser_core::EngineEvent::Quit {}
        })
        .await
        .expect("the session engine was asked to quit");
    });
    drop(f);
}

#[test]
fn tray_quit_in_ui_mode_with_no_session_engine_still_exits() {
    let dir = tempfile::tempdir().unwrap();
    let app = tauri::async_runtime::block_on(async {
        let gone = dir.path().join("no-engine");
        app_with(crate::host_tests::remote::ui_host_at(&gone).await)
    });
    let exited = Cell::new(false);
    quit(app.handle(), || exited.set(true));
    assert!(exited.get());
}
