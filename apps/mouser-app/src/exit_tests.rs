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
