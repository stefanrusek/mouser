use mouser_proto::control::ConnectionState;
use mouser_proto::control::Method;
use mouser_proto::model::{PermissionKind, PermissionState};
use serde_json::json;
use tauri::{Listener, Manager};

use crate::support_tests::{app_with, fixture, status_of};
use crate::windows::{is_shown, navigate, needs_setup, open_home};

#[tokio::test(flavor = "multi_thread")]
async fn setup_is_unfinished_off_a_desk_or_with_a_missing_permission() {
    let f = fixture().await;
    let mut status = status_of(&f.host).await;
    assert!(status.desk.is_none());
    assert!(needs_setup(&status));

    f.host
        .control()
        .call(Method::DeskCreate, Some(json!({"name": "Home office"})))
        .await
        .unwrap();
    status = status_of(&f.host).await;
    assert!(status.desk.is_some());
    assert!(!needs_setup(&status));

    status.computers[0]
        .permissions
        .insert(PermissionKind::Accessibility, PermissionState::Missing);
    assert!(needs_setup(&status));

    status.computers[0].connection = ConnectionState::Connected;
    assert!(
        !needs_setup(&status),
        "only this computer's permissions count"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn open_home_goes_to_setup_first_then_the_desk() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let control = { app.state::<crate::host::Host>().control() };
    open_home(&handle, &control).await;
    let window = handle.get_webview_window("main").unwrap();
    assert!(
        window
            .url()
            .unwrap()
            .as_str()
            .ends_with("index.html#welcome")
    );
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    window.listen("mouser://navigate", move |e| {
        sink.lock().unwrap().push(e.payload().to_string())
    });
    control
        .call(Method::DeskCreate, Some(json!({"name": "Home office"})))
        .await
        .unwrap();
    open_home(&handle, &control).await;
    crate::support_tests::eventually(|| !seen.lock().unwrap().is_empty()).await;
    assert_eq!(seen.lock().unwrap()[0], r##"{"route":"#desk"}"##);
    assert_eq!(handle.webview_windows().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_window_counts_as_shown_once_navigated_to() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!is_shown(&handle));
    navigate(&handle, "#desk");
    assert!(is_shown(&handle));
}
