use mouser_proto::control::ConnectionState;
use mouser_proto::control::Method;
use mouser_proto::model::{PermissionKind, PermissionState};
use serde_json::json;
use tauri::Manager;

use crate::support_tests::{app_with, fixture, has_window, status_of};
use crate::windows::{Which, needs_setup, open_home, show};

#[test]
fn each_window_has_its_own_page_label_and_title() {
    for (which, label, page, title) in [
        (
            Which::FirstRun,
            "main",
            "first_run.html",
            "Welcome to Mouser",
        ),
        (Which::Settings, "settings", "desk_settings.html", "Mouser"),
        (
            Which::Join,
            "join",
            "join_prompt.html",
            "A computer wants to join",
        ),
        (
            Which::Arranger,
            "arranger",
            "arranger.html",
            "Arrange displays",
        ),
        (
            Which::Transfer,
            "transfer",
            "transfer.html",
            "File transfer",
        ),
        (Which::Health, "health", "health.html", "Mouser health"),
    ] {
        assert_eq!(which.label(), label);
        assert_eq!(which.page(), page);
        assert_eq!(which.title(), title);
        let (w, h) = which.size();
        assert!(w > 0.0 && h > 0.0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn show_creates_a_closed_window_and_reuses_an_open_one() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!has_window(&handle, "main"));
    show(&handle, Which::FirstRun);
    assert!(has_window(&handle, "main"));
    assert!(!has_window(&handle, "join"));
    show(&handle, Which::Join);
    show(&handle, Which::Join);
    assert!(has_window(&handle, "join"));
    assert_eq!(handle.webview_windows().len(), 2);
}

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
async fn open_home_shows_setup_first_then_the_settings() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let control = { app.state::<crate::host::Host>().control() };
    open_home(&handle, &control).await;
    assert!(has_window(&handle, "main"));
    assert!(!has_window(&handle, "settings"));
    control
        .call(Method::DeskCreate, Some(json!({"name": "Home office"})))
        .await
        .unwrap();
    open_home(&handle, &control).await;
    assert!(has_window(&handle, "settings"));
}
