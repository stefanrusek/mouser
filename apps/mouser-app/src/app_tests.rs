use std::sync::{Arc, Mutex};

use mouser_proto::control::{Method, Notification, Topic};
use serde_json::json;
use tauri::{Listener, Manager};

use crate::app::{EVENT, handle, handle_event, pump, pump_events, start};
use crate::notify_tests::problem_note;
use crate::support_tests::{
    FakeNotifier, FakeTray, app_with, eventually, fixture, has_window, status_of,
};
use crate::tray::{Appearance, TrayController};
use mouser_platform::{Os, Platform};

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
    assert!(has_window(&handle_, "main"));

    handle(&handle_, &tray, &notifier, problem_note(false));
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
    // Cleared and raised again at once: it was just shown, so not again.
    handle(&handle_, &tray, &notifier, problem_note(true));
    handle(&handle_, &tray, &notifier, problem_note(false));
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);

    handle(&handle_, &tray, &notifier, transfer_note("copying"));
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
    assert_eq!(port.views.lock().unwrap().len(), 1);
    assert!(has_window(&handle_, "main"));

    eventually(|| *events.lock().unwrap() == 6).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_config_notification_reaches_the_windows_and_registers_hotkeys_again() {
    let f = fixture().await;
    let set = crate::hotkeys_tests::hotkeys_for(f.host.control());
    set.hotkeys.sync();
    set.config.lock().unwrap().hotkeys.enable_disable = "Ctrl+Alt+P".into();
    let app = app_with(f.host);
    app.manage(set.hotkeys.clone());
    let handle_ = app.handle().clone();
    let port = FakeTray::default();
    let tray = TrayController::new(Box::new(port.clone()), Os::MacOs, Appearance::Light);
    let notifier = FakeNotifier::default();
    let events = Arc::new(std::sync::Mutex::new(0));
    let seen = events.clone();
    handle_.listen(EVENT, move |_| *seen.lock().unwrap() += 1);

    handle(
        &handle_,
        &tray,
        &notifier,
        note(
            Topic::Config,
            json!({"key": "hotkeys.enable_disable", "old": "a", "new": "b", "origin": "cli"}),
        ),
    );
    eventually(|| *events.lock().unwrap() == 1).await;
    assert_eq!(set.shortcuts.replaced.lock().unwrap().len(), 2);
    assert!(port.views.lock().unwrap().is_empty());
    assert!(notifier.shown.lock().unwrap().is_empty());
}

fn transfer_note(state: &str) -> Notification {
    note(
        Topic::Transfer,
        json!({"transfer_id": "ab12", "state": state}),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn transfer_progress_opens_the_window_once_and_a_question_brings_it_forward() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let (tray, _port, notifier, _dynamic) = pump_parts();

    handle(&handle_, &tray, &notifier, transfer_note("completed"));
    assert!(
        !has_window(&handle_, "main"),
        "a finished copy needs no window"
    );

    handle(&handle_, &tray, &notifier, transfer_note("pending"));
    assert!(has_window(&handle_, "main"));
    handle(&handle_, &tray, &notifier, transfer_note("copying"));
    assert_eq!(handle_.webview_windows().len(), 1);

    handle(
        &handle_,
        &tray,
        &notifier,
        transfer_note("awaiting_overwrite_decision"),
    );
    assert!(has_window(&handle_, "main"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_or_failed_transfer_opens_the_window_and_is_kept_for_a_window_that_opens_late() {
    for state in ["rejected", "failed"] {
        let f = fixture().await;
        let app = app_with(f.host);
        let handle_ = app.handle().clone();
        let (tray, _port, notifier, _dynamic) = pump_parts();
        let stopped = note(
            Topic::Transfer,
            json!({"transfer_id": "ab12", "state": state, "message": "It stopped."}),
        );
        handle(&handle_, &tray, &notifier, transfer_note("copying"));
        handle(&handle_, &tray, &notifier, stopped.clone());
        assert!(has_window(&handle_, "main"));
        // The window was created after the first notification; it still gets the last word.
        assert_eq!(
            handle_.state::<crate::host::Host>().transfers(),
            vec![stopped.params]
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_one_shot_rejection_is_kept_even_though_no_window_was_open() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let (tray, _port, notifier, _dynamic) = pump_parts();
    assert!(!has_window(&handle_, "main"));
    handle(&handle_, &tray, &notifier, transfer_note("rejected"));
    assert!(has_window(&handle_, "main"));
    assert_eq!(handle_.state::<crate::host::Host>().transfers().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_app_shows_setup_and_the_tray_then_follows_the_engine() {
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
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
async fn falling_behind_the_engine_resyncs_the_tray_and_windows_from_status() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let seen = Arc::new(std::sync::Mutex::new(0));
    let sink = seen.clone();
    handle_.listen(EVENT, move |_| *sink.lock().unwrap() += 1);
    let (tray, port, notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(1);
    for _ in 0..3 {
        sender.send(problem_note(false)).unwrap();
    }
    drop(sender);
    pump(handle_, tray, dynamic, receiver).await;
    // The tray was redrawn from the engine's status, not from a lost note.
    assert_eq!(port.views.lock().unwrap().len(), 1);
    // The windows got that status, then the one note still in the channel.
    eventually(|| *seen.lock().unwrap() == 2).await;
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
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
    assert!(has_window(&handle_, "main"));
}

#[tokio::test(flavor = "multi_thread")]
async fn quickjump_letters_show_on_this_computers_displays_and_hide_again() {
    let f = fixture().await;
    let own = status_of(&f.host).await.computer.id;
    let key = f.platform.displays().unwrap()[0].key.clone();
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let notifier = FakeNotifier::default();
    let letters = vec![(mouser_proto::model::DisplayId { computer: own, key }, 'A')];
    handle_event(
        &handle_,
        &notifier,
        mouser_core::EngineEvent::QuickJumpShow { letters },
    );
    let count = || {
        handle_
            .webview_windows()
            .keys()
            .filter(|label| label.starts_with("quickjump-"))
            .count()
    };
    eventually(|| count() == 1).await;
    let ended = Arc::new(Mutex::new(0));
    for (label, window) in handle_.webview_windows() {
        if label.starts_with("quickjump-") {
            let sink = ended.clone();
            window.listen(crate::quickjump_windows::EVENT, move |_| {
                *sink.lock().unwrap() += 1
            });
        }
    }
    handle_event(&handle_, &notifier, mouser_core::EngineEvent::QuickJumpHide);
    eventually(|| *ended.lock().unwrap() == 1).await;
    assert!(notifier.shown.lock().unwrap().is_empty());
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
async fn falling_behind_on_engine_events_carries_on_with_the_ones_still_there() {
    let f = fixture().await;
    let app = app_with(f.host);
    let (_tray, _port, notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(1);
    for _ in 0..3 {
        sender.send(review()).unwrap();
    }
    drop(sender);
    pump_events(app.handle().clone(), dynamic, receiver).await;
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
}

// v2 placeholder: no Phase 9 task owns this (core logs the fault; the app has nothing to add)
#[tokio::test(flavor = "multi_thread")]
async fn a_fault_in_the_engine_needs_nothing_from_the_app() {
    let f = fixture().await;
    let app = app_with(f.host);
    let notifier = FakeNotifier::default();
    handle_event(
        app.handle(),
        &notifier,
        mouser_core::EngineEvent::Fault {
            r#where: "reconcile".into(),
        },
    );
    assert!(notifier.shown.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pump_shows_no_system_notification_when_notifications_are_off() {
    let f = fixture().await;
    f.host
        .control()
        .call(
            Method::ConfigSet,
            Some(json!({"key": "general.notifications", "value": false})),
        )
        .await
        .unwrap();
    let app = app_with(f.host);
    let (tray, _port, notifier, dynamic) = pump_parts();
    let (sender, receiver) = tokio::sync::broadcast::channel(4);
    sender.send(problem_note(false)).unwrap();
    drop(sender);
    pump(app.handle().clone(), tray, dynamic, receiver).await;
    assert!(notifier.shown.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_the_app_installs_the_panic_hook_on_the_engines_log() {
    let _one_at_a_time = crate::support_tests::one_at_a_time().await;
    let f = fixture().await;
    let dir = f._dir.path().to_path_buf();
    let app = app_with(f.host);
    let handle_ = app.handle().clone();
    let control = app.state::<crate::host::Host>().control();
    control
        .call(Method::LoggingSet, Some(json!({"enabled": true})))
        .await
        .unwrap();
    let (tray, _port, _notifier, _dynamic) = pump_parts();
    start(&handle_, &control, &tray).await;
    crate::support_tests::panic_on_thread("app-t231", "app went wrong");
    let records = crate::support_tests::log_records(&dir);
    let fault = records
        .iter()
        .find(|r| r["event"] == "fault" && r["fields"]["message"] == "app went wrong")
        .unwrap_or_else(|| panic!("a fault record: {records:?}"));
    assert_eq!(fault["fields"]["where"], "app-t231");
}

/// A Windows that answers the restart as administrator with `answer`.
#[cfg(target_os = "windows")]
struct AnsweringShell {
    answer: crate::elevation::RunAs,
    accept: bool,
    started: std::sync::Mutex<Vec<String>>,
}

#[cfg(target_os = "windows")]
impl crate::elevation::Shell for AnsweringShell {
    fn start(
        &self,
        _exe: &std::path::Path,
        args: &str,
        as_admin: bool,
    ) -> Result<(), crate::elevation::RunAs> {
        assert!(as_admin);
        self.started.lock().unwrap().push(args.to_string());
        if self.accept {
            Ok(())
        } else {
            Err(self.answer.clone())
        }
    }

    fn wait_for_process(&self, _pid: u32) {}

    fn process_alive(&self, _pid: u32) -> bool {
        false
    }

    fn message_box(&self, _text: &str) {}
}

#[cfg(target_os = "windows")]
fn shell_that(accept: bool) -> AnsweringShell {
    AnsweringShell {
        answer: crate::elevation::RunAs::Declined,
        accept,
        started: std::sync::Mutex::new(Vec::new()),
    }
}

#[cfg(target_os = "windows")]
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_request_whose_prompt_is_declined_leaves_mouser_running() {
    use std::sync::Arc;
    let f = fixture().await;
    let path = crate::support_tests::control_path_in(&f._dir);
    let app = app_with(f.host);
    app.state::<crate::host::Host>()
        .control()
        .call(Method::LoggingSet, Some(json!({"enabled": true})))
        .await
        .unwrap();
    let shell = Arc::new(shell_that(false));
    app.manage(crate::app::ElevationShell {
        shell: shell.clone(),
        exit: Arc::new(|| panic!("a declined restart does not exit")),
    });
    let notifier = FakeNotifier::default();
    handle_event(
        app.handle(),
        &notifier,
        mouser_core::EngineEvent::RestartElevated,
    );
    let dir = f._dir.path().to_path_buf();
    let outcomes = move || -> Vec<serde_json::Value> {
        crate::support_tests::log_records(&dir)
            .iter()
            .filter(|r| r["event"] == "elevation.restart")
            .map(|r| r["fields"]["outcome"].clone())
            .collect()
    };
    // The answer is logged after Windows answered.
    crate::support_tests::eventually(|| outcomes().len() == 2).await;
    assert_eq!(outcomes(), ["prompted", "declined"]);
    let pid = std::process::id();
    assert_eq!(
        *shell.started.lock().unwrap(),
        [format!("--wait-for {pid}")]
    );
    assert!(path.exists(), "the engine runs on");
}

#[cfg(target_os = "windows")]
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_request_whose_prompt_is_accepted_shuts_the_engine_down_then_exits() {
    use std::sync::Arc;
    let f = fixture().await;
    let path = crate::support_tests::control_path_in(&f._dir);
    let app = app_with(f.host);
    let exited_after_shutdown = Arc::new(Mutex::new(None));
    let seen = exited_after_shutdown.clone();
    let gone = path.clone();
    app.manage(crate::app::ElevationShell {
        shell: Arc::new(shell_that(true)),
        exit: Arc::new(move || *seen.lock().unwrap() = Some(!gone.exists())),
    });
    handle_event(
        app.handle(),
        &FakeNotifier::default(),
        mouser_core::EngineEvent::RestartElevated,
    );
    crate::support_tests::eventually(|| exited_after_shutdown.lock().unwrap().is_some()).await;
    assert_eq!(*exited_after_shutdown.lock().unwrap(), Some(true));
}

#[cfg(target_os = "windows")]
#[test]
fn an_accepted_restart_shuts_the_engine_down_then_exits() {
    let (app, path, _dir) = tauri::async_runtime::block_on(async {
        let f = fixture().await;
        let path = crate::support_tests::control_path_in(&f._dir);
        (app_with(f.host), path, f._dir)
    });
    let exited_after_shutdown = std::cell::Cell::new(None);
    let state = crate::app::restart_elevated(app.handle(), &shell_that(true), || {
        exited_after_shutdown.set(Some(!path.exists()));
    });
    assert_eq!(state, crate::elevation::State::HandingOver);
    assert_eq!(exited_after_shutdown.get(), Some(true));
}

#[cfg(not(target_os = "windows"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_request_off_windows_changes_nothing() {
    // Only a portable Windows computer is asked to restart elevated.
    let f = fixture().await;
    let app = app_with(f.host);
    let notifier = FakeNotifier::default();
    handle_event(
        app.handle(),
        &notifier,
        mouser_core::EngineEvent::RestartElevated,
    );
    assert!(notifier.shown.lock().unwrap().is_empty());
    assert!(app.handle().webview_windows().is_empty());
}
