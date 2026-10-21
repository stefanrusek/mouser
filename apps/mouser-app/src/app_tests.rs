use std::sync::Arc;

use mouser_proto::control::{Method, Notification, Topic};
use serde_json::json;
use tauri::{Listener, Manager};

use crate::app::{EVENT, handle, handle_event, pump, pump_events, start};
use crate::notify_tests::problem_note;
use crate::support_tests::{
    FakeNotifier, FakeTray, app_with, eventually, fixture, has_window, status_of,
};
use crate::tray::{Appearance, TrayController};
use mouser_platform::Os;

fn note(method: Topic, params: serde_json::Value) -> Notification {
    Notification {
        jsonrpc: "2.0".into(),
        method,
        params,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn each_notification_reaches_the_windows_and_its_consumer() {
    let f = fixture().await;
    let status = status_of(&f.host).await;
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let port = FakeTray::default();
    let tray = TrayController::new(Box::new(port.clone()), Os::MacOs, Appearance::Light);
    let notifier = FakeNotifier::default();

    let events = Arc::new(std::sync::Mutex::new(0));
    let seen = events.clone();
    handle_.listen(EVENT, move |_| *seen.lock().unwrap() += 1);

    let status_note = note(Topic::Status, serde_json::to_value(&status).unwrap());
    handle(&handle_, &tray, &notifier, status_note);
    assert_eq!(port.views.lock().unwrap().len(), 1);

    handle(
        &handle_,
        &tray,
        &notifier,
        note(Topic::JoinRequest, json!({})),
    );
    assert!(has_window(&handle_, "join"));

    handle(&handle_, &tray, &notifier, problem_note(false));
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);

    handle(&handle_, &tray, &notifier, note(Topic::Transfer, json!({})));
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
    assert_eq!(port.views.lock().unwrap().len(), 1);

    eventually(|| *events.lock().unwrap() == 4).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_app_shows_setup_and_the_tray_then_follows_the_engine() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let control = app.state::<crate::host::Host>().control();
    let (tray, port, _notifier, dynamic) = pump_parts();

    let notes = start(&handle_, &control, &tray).await;
    assert!(has_window(&handle_, "main"));
    assert_eq!(port.views.lock().unwrap().len(), 1);

    tauri::async_runtime::spawn(pump(handle_.clone(), tray, dynamic, notes));
    // Creating a desk changes the status, which the engine announces.
    control
        .call(Method::DeskCreate, Some(json!({"name": "Home office"})))
        .await
        .unwrap();
    eventually(|| port.views.lock().unwrap().len() >= 2).await;
}

fn pump_parts() -> (
    Arc<TrayController>,
    FakeTray,
    FakeNotifier,
    Arc<dyn crate::notify::Notifier>,
) {
    let port = FakeTray::default();
    let tray = Arc::new(TrayController::new(
        Box::new(port.clone()),
        Os::MacOs,
        Appearance::Light,
    ));
    let notifier = FakeNotifier::default();
    let dynamic: Arc<dyn crate::notify::Notifier> = Arc::new(notifier.clone());
    (tray, port, notifier, dynamic)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pump_ends_when_the_engine_stops() {
    let f = fixture().await;
    let app = app_with(f.host);
    let (tray, _port, notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(4);
    sender.send(problem_note(false)).unwrap();
    drop(sender);
    pump(app.handle().clone(), tray, dynamic, receiver).await;
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "the app fell 2 notifications behind the engine")]
async fn falling_behind_the_engine_is_a_bug() {
    let f = fixture().await;
    let app = app_with(f.host);
    let (tray, _port, _notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(1);
    for _ in 0..3 {
        sender.send(problem_note(false)).unwrap();
    }
    pump(app.handle().clone(), tray, dynamic, receiver).await;
}

fn landing() -> mouser_core::EngineEvent {
    mouser_core::EngineEvent::Landing {
        display: mouser_proto::model::DisplayId {
            computer: mouser_proto::ids::ComputerId([1; 16]),
            key: "display-1".into(),
        },
        x: 10.0,
        y: 20.0,
        desktop: (1000.0, 500.0),
    }
}

fn review() -> mouser_core::EngineEvent {
    mouser_core::EngineEvent::ReviewOffered {
        computer: mouser_proto::ids::ComputerId([2; 16]),
        name: "Studio".into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_landing_flashes_the_overlay_and_a_review_offer_is_a_notification() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    crate::overlay::create(&handle_);
    let seen = Arc::new(std::sync::Mutex::new(0));
    let sink = seen.clone();
    handle_
        .get_webview_window(crate::overlay::LABEL)
        .unwrap()
        .listen(crate::overlay::EVENT, move |_| *sink.lock().unwrap() += 1);
    let notifier = FakeNotifier::default();

    handle_event(&handle_, &notifier, landing());
    eventually(|| *seen.lock().unwrap() == 1).await;
    assert!(notifier.shown.lock().unwrap().is_empty());

    handle_event(&handle_, &notifier, review());
    assert_eq!(
        *notifier.shown.lock().unwrap(),
        vec![(
            "Check where Studio's screens sit".to_string(),
            "Studio joined your desk. Open Mouser to review where its displays are placed."
                .to_string()
        )]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_event_pump_ends_when_the_engine_stops() {
    let f = fixture().await;
    let app = app_with(f.host);
    let (_tray, _port, notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(4);
    sender.send(review()).unwrap();
    drop(sender);
    pump_events(app.handle().clone(), dynamic, receiver).await;
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "the app fell 2 engine events behind the engine")]
async fn falling_behind_on_engine_events_is_a_bug() {
    let f = fixture().await;
    let app = app_with(f.host);
    let (_tray, _port, _notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(1);
    for _ in 0..3 {
        sender.send(review()).unwrap();
    }
    pump_events(app.handle().clone(), dynamic, receiver).await;
}
