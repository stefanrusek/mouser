use mouser_platform::{Os, Platform};
use mouser_proto::control::{Method, Notification, Topic};
use tauri::Manager;

use crate::host::Host;

use crate::host::on_second_launch;
use crate::support_tests::{app_with, fixture, has_window, status_of};

#[tokio::test(flavor = "multi_thread")]
async fn the_host_runs_an_engine_on_the_given_platform_and_exposes_the_control_api() {
    let f = fixture().await;
    let status = status_of(&f.host).await;
    assert_eq!(status.computer.name, "Desk Mac");
    assert_eq!(status.version, "1.2.3");
    let desks = f.host.control().call(Method::DesksDiscovered, None).await;
    assert_eq!(desks.unwrap(), serde_json::json!([]));
    assert_eq!(f.host.platform().info().os, Os::Linux);
    f.host
        .platform()
        .open_permission_settings(mouser_platform::PermissionKind::Accessibility)
        .unwrap();
    assert_eq!(
        f.platform.opened_permission_settings(),
        vec![mouser_platform::PermissionKind::Accessibility]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_latest_notification_of_each_transfer_is_kept_in_first_seen_order() {
    let f = fixture().await;
    assert!(f.host.transfers().is_empty());
    let note = |id: &str, state: &str| serde_json::json!({"transfer_id": id, "state": state});
    f.host.remember_transfer(note("a", "copying"));
    f.host.remember_transfer(note("b", "rejected"));
    f.host.remember_transfer(note("a", "completed"));
    assert_eq!(
        f.host.transfers(),
        vec![note("a", "completed"), note("b", "rejected")]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_edge_asked_for_is_handed_over_once() {
    let f = fixture().await;
    assert_eq!(f.host.take_edge(), None);
    f.host.ask_for_edge("crossing_blocked:aa/1/right".into());
    f.host.ask_for_edge("crossing_blocked:aa/1/left".into());
    assert_eq!(
        f.host.take_edge(),
        Some("crossing_blocked:aa/1/left".to_string())
    );
    assert_eq!(f.host.take_edge(), None);
}

/// The route the one window was opened at.
fn opened_at(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> String {
    let window = app.get_webview_window("main").expect("main exists");
    window
        .url()
        .unwrap()
        .fragment()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_launch_brings_up_the_desk() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!has_window(&handle, "main"));
    on_second_launch(&handle);
    assert_eq!(opened_at(&handle), "desk");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_join_request_and_a_transfer_question_open_the_desk() {
    use crate::app::handle as handle_note;
    use crate::support_tests::{FakeNotifier, FakeTray};
    use crate::tray::{Appearance, TrayController};
    for (topic, params) in [
        (Topic::JoinRequest, serde_json::json!({})),
        (
            Topic::Transfer,
            serde_json::json!({"transfer_id": "ab", "state": "awaiting_overwrite_decision"}),
        ),
    ] {
        let f = fixture().await;
        let app = app_with(f.host);
        let handle = app.handle().clone();
        let tray = TrayController::new(Box::new(FakeTray::default()), Os::MacOs, Appearance::Light);
        let note = Notification {
            jsonrpc: "2.0".into(),
            method: topic,
            params,
        };
        handle_note(&handle, &tray, &FakeNotifier::default(), note);
        assert_eq!(opened_at(&handle), "desk");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_offer_and_the_crossing_blocked_fix_open_the_displays() {
    use mouser_proto::control::FixAction;
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    crate::app::handle_event(
        &handle,
        &crate::support_tests::FakeNotifier::default(),
        mouser_core::EngineEvent::ReviewOffered {
            computer: mouser_proto::ids::ComputerId([2; 16]),
            name: "Studio".into(),
        },
    );
    assert_eq!(opened_at(&handle), "displays");

    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let host = handle.state::<Host>();
    let _ = crate::commands::run_fix(
        &handle,
        &host,
        "crossing_blocked:aa/1/right",
        FixAction::OpenArrangerEdge,
    );
    assert_eq!(opened_at(&handle), "displays");
}

#[tokio::test(flavor = "multi_thread")]
async fn exporting_the_machine_scope_copies_the_identity_and_the_state() {
    let f = fixture().await;
    let machine = tempfile::tempdir().unwrap();
    let mut store = mouser_core::engine_headless::FileSecrets::new(machine.path().join("secrets"));
    f.host.export_machine_scope(machine.path(), &mut store);
    let key = f
        .platform
        .secrets()
        .get(mouser_core::identity::DEVICE_KEY)
        .unwrap()
        .unwrap();
    assert_eq!(
        mouser_platform::SecretStore::get(&store, mouser_core::identity::DEVICE_KEY).unwrap(),
        Some(key)
    );
    assert!(machine.path().join(mouser_core::state::FILE).exists());
}

#[test]
fn shutting_the_host_down_stops_the_engine_once_however_often_it_is_asked() {
    tauri::async_runtime::block_on(async {
        let f = fixture().await;
        let path = crate::support_tests::control_path_in(&f._dir);
        assert!(path.exists());
        f.host.shutdown().await;
        assert!(!path.exists());
        f.host.shutdown().await;
        assert!(!path.exists());
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_starts_the_engine_in_the_mode_it_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(crate::support_tests::free_port());
    mouser_core::config::save(dir.path(), &config);
    let host = crate::host::Host::start(
        std::sync::Arc::new(mouser_testkit::VirtualPlatform::new()),
        dir.path().to_path_buf(),
        crate::support_tests::control_path_in(&dir),
        "Desk Mac".into(),
        "1.2.3".into(),
        mouser_proto::wire::ComputerMode::PortableElevated,
        crate::host::Updates::default(),
    )
    .await;
    let status = status_of(&host).await;
    let own = status
        .computers
        .iter()
        .find(|c| c.id == status.computer.id)
        .unwrap();
    assert_eq!(
        own.mode,
        Some(mouser_proto::wire::ComputerMode::PortableElevated)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_toast_names_the_computer_from_the_latest_status() {
    use mouser_proto::control::{Problem, ProblemKind};
    let f = fixture().await;
    let status = status_of(&f.host).await;
    f.host.remember_names(&status);
    let problem = Problem {
        problem_id: "peer_offline:x".into(),
        kind: ProblemKind::PeerOffline,
        computer: status.computer.id,
        message: "Offline.".into(),
        app: None,
        fix: None,
        raised_at: "2026-10-21T00:00:00Z".into(),
        cleared_at: None,
    };
    let note = Notification {
        jsonrpc: "2.0".into(),
        method: Topic::Problem,
        params: serde_json::to_value(problem).unwrap(),
    };
    let notifier = crate::support_tests::FakeNotifier::default();
    f.host.toast(&note, &notifier);
    assert_eq!(
        notifier.shown.lock().unwrap()[0].0,
        "Mouser needs your attention on Desk Mac"
    );
}

/// The UI-mode client of another engine's control API (research V9).
pub(crate) mod remote {
    use std::sync::Arc;

    use mouser_proto::control::{Method, Topic};
    use mouser_testkit::VirtualPlatform;
    use serde_json::json;

    use crate::commands::LogsLink;
    use crate::host::{Control, Host, NOT_YET};
    use crate::support_tests::{control_path_in, fixture};

    /// A UI-mode host talking to the engine of `f` (standing in for the
    /// session engine behind the service pipe).
    pub(crate) async fn ui_host(f: &crate::support_tests::Fixture) -> Host {
        let log_dir = tempfile::tempdir().unwrap();
        Host::connect(
            &control_path_in(&f._dir),
            Arc::new(VirtualPlatform::new()),
            log_dir.path(),
        )
        .await
        .expect("the engine's control API is reachable")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_ui_mode_host_has_no_engine_of_its_own_and_talks_to_the_one_behind_the_pipe() {
        let f = fixture().await;
        let ui = ui_host(&f).await;
        assert!(matches!(ui.control(), Control::Remote(_)));
        let status = ui.control().call(Method::Status, None).await.unwrap();
        let own = f.host.control().call(Method::Status, None).await.unwrap();
        assert_eq!(
            status["computer"]["id"], own["computer"]["id"],
            "the same engine"
        );
        // Its notifications arrive here.
        let mut notes = ui.control().notifications();
        f.host
            .control()
            .call(Method::LoggingSet, Some(json!({"enabled": true})))
            .await
            .unwrap();
        let note = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let note = notes.recv().await.unwrap();
                if note.method == Topic::Status {
                    return note;
                }
            }
        })
        .await
        .expect("a status notification arrives");
        assert_eq!(note.method, Topic::Status);
        // Shutting the UI down leaves the engine it talks to running.
        ui.shutdown().await;
        assert!(control_path_in(&f._dir).exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn what_the_service_engine_cannot_do_for_the_ui_yet_is_an_error_not_silence() {
        // v2: replaced by T532 (problem.fix, transfer.resolve, the engine_event topic).
        let f = fixture().await;
        let ui = ui_host(&f).await;
        assert_eq!(ui.control().fix("some-problem"), Err(NOT_YET.to_string()));
        assert_eq!(
            ui.control().resolve_transfer("some-transfer", true),
            Err(NOT_YET.to_string())
        );
        let mut events = ui.control().events();
        assert!(matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_remote_client_links_the_logs_of_the_engine_it_talks_to() {
        let f = fixture().await;
        let ui = ui_host(&f).await;
        let Control::Remote(remote) = ui.control() else {
            panic!("a UI-mode host is remote");
        };
        let own = f.host.control().call(Method::Status, None).await.unwrap();
        let status = remote.status().await.unwrap();
        assert_eq!(
            serde_json::to_value(status.computer.id).unwrap(),
            own["computer"]["id"]
        );
        let streamed = remote.collect_stream().await.unwrap();
        // Every chunk came before the result, in order.
        assert!(!streamed.chunks.is_empty());
        assert!(
            streamed
                .chunks
                .iter()
                .enumerate()
                .all(|(i, c)| c.seq as usize == i)
        );
        assert!(streamed.result.size > 0);
        assert!(remote.collect().await.is_ok());
    }
}

/// Events reach an app with its own engine.
#[tokio::test(flavor = "multi_thread")]
async fn a_local_host_hands_out_its_engines_events() {
    let f = crate::support_tests::fixture().await;
    let mut events = f.host.control().events();
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn when_the_engine_behind_the_pipe_goes_the_ui_hears_no_more() {
    let f = crate::support_tests::fixture().await;
    let ui = remote::ui_host(&f).await;
    let mut notes = ui.control().notifications();
    f.host.shutdown().await;
    let ended = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Err(tokio::sync::broadcast::error::RecvError::Closed) = notes.recv().await {
                return;
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "its notifications ended");
    // Asked afterwards, there is nothing to hear either.
    assert!(matches!(
        ui.control().notifications().recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Closed)
    ));
}
