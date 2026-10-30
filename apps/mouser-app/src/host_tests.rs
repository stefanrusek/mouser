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
    )
    .await;
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
    use crate::host::{Control, Host, NOT_READY};
    use crate::support_tests::{control_path_in, fixture};

    /// A UI-mode host talking to the engine of `f` (standing in for the
    /// session engine behind the service pipe).
    pub(crate) async fn ui_host(f: &crate::support_tests::Fixture) -> Host {
        ui_host_at(&control_path_in(&f._dir)).await
    }

    /// A UI-mode host on the control API at `path`, once it is ready when
    /// an engine is there.
    pub(crate) async fn ui_host_at(path: &std::path::Path) -> Host {
        let log_dir = tempfile::tempdir().unwrap();
        let ui = Host::connect(path, Arc::new(VirtualPlatform::new()), log_dir.path());
        if path.exists() {
            until_ready(&ui, true).await;
        }
        ui
    }

    /// Waits until `ui` is `ready` (or not).
    pub(crate) async fn until_ready(ui: &Host, ready: bool) {
        let mut seen = ui.control().ready();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while *seen.borrow_and_update() != ready {
                seen.changed().await.unwrap();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("never ready = {ready}"));
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
    async fn a_fix_in_ui_mode_is_carried_out_by_the_session_engine() {
        let f = fixture().await;
        let id = crate::commands_tests::missing_permission_problem(&f).await;
        let ui = ui_host(&f).await;
        ui.control().fix(&id).await.unwrap();
        assert_eq!(
            f.platform.opened_permission_settings(),
            vec![mouser_platform::PermissionKind::Accessibility]
        );
        assert_eq!(
            ui.control().fix("nope").await,
            Err("nope is not an open problem".to_string())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fix_or_an_answer_while_the_session_engine_is_away_says_it_is_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        let ui = ui_host_at(&control_path_in(&dir)).await;
        assert_eq!(ui.control().fix("x").await, Err(NOT_READY.to_string()));
        assert_eq!(
            ui.control().resolve_transfer("t", true).await,
            Err(NOT_READY.to_string())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_answer_about_a_transfer_the_session_engine_does_not_know_is_ignored() {
        let f = fixture().await;
        let ui = ui_host(&f).await;
        assert_eq!(
            ui.control().resolve_transfer("some-transfer", true).await,
            Ok(())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_session_engines_events_reach_the_ui() {
        // A portable Windows engine is asked to restart itself as
        // administrator: the app in UI mode is who runs that sequence.
        let dir = tempfile::tempdir().unwrap();
        let mut config = mouser_proto::config::Config::default();
        config.advanced.port = u32::from(crate::support_tests::free_port());
        mouser_core::config::save(dir.path(), &config);
        let platform = VirtualPlatform::new();
        let mut info = mouser_platform::Platform::info(&platform);
        info.os = mouser_platform::Os::Windows;
        platform.set_info(info);
        let path = control_path_in(&dir);
        let engine = Host::start(
            Arc::new(platform),
            dir.path().to_path_buf(),
            path.clone(),
            "Portable PC".into(),
            "1.2.3".into(),
            mouser_proto::wire::ComputerMode::Portable,
            crate::host::Updates::default(),
        )
        .await;
        let ui = ui_host_at(&path).await;
        let mut events = ui.control().events();
        let me = engine.control().call(Method::Status, None).await.unwrap();
        ui.control()
            .call(
                Method::ElevationRestart,
                Some(json!({"computer": me["computer"]["id"]})),
            )
            .await
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the event arrives")
            .unwrap();
        assert_eq!(event, mouser_core::EngineEvent::RestartElevated);
    }

    #[test]
    fn every_engine_event_the_session_engine_sends_is_the_apps_own() {
        use mouser_core::EngineEvent as App;
        use mouser_proto::control::EngineEvent as Wire;
        use mouser_proto::ids::ComputerId;
        use mouser_proto::model::DisplayId;
        let display = DisplayId {
            computer: ComputerId([5; 16]),
            key: "DP-1".into(),
        };
        let cases = [
            (
                Wire::Landing {
                    display: display.clone(),
                    x: 1.0,
                    y: 2.0,
                    desktop: (3.0, 4.0),
                },
                App::Landing {
                    display: display.clone(),
                    x: 1.0,
                    y: 2.0,
                    desktop: (3.0, 4.0),
                },
            ),
            (
                Wire::ReviewOffered {
                    computer: ComputerId([2; 16]),
                    name: "Studio".into(),
                },
                App::ReviewOffered {
                    computer: ComputerId([2; 16]),
                    name: "Studio".into(),
                },
            ),
            (
                Wire::QuickJumpShow {
                    letters: vec![(display.clone(), 'A')],
                },
                App::QuickJumpShow {
                    letters: vec![(display, 'A')],
                },
            ),
            (Wire::QuickJumpHide, App::QuickJumpHide),
            (Wire::RestartElevated, App::RestartElevated),
        ];
        for (wire, app) in cases {
            assert_eq!(crate::host::engine_event(wire), app);
        }
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

/// An engine at `path` with a config of its own in `dir`.
async fn engine_at(path: &std::path::Path, dir: &std::path::Path) -> crate::host::Host {
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(crate::support_tests::free_port());
    mouser_core::config::save(dir, &config);
    crate::host::Host::start(
        std::sync::Arc::new(mouser_testkit::VirtualPlatform::new()),
        dir.to_path_buf(),
        path.to_path_buf(),
        "Session engine".into(),
        "1.2.3".into(),
        mouser_proto::wire::ComputerMode::Installed,
        crate::host::Updates::default(),
    )
    .await
}

/// The first `status` notification `notes` hears after `trigger` runs.
async fn a_status_after(
    notes: &mut tokio::sync::broadcast::Receiver<mouser_proto::control::Notification>,
    trigger: impl std::future::Future<Output = ()>,
) {
    trigger.await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while notes.recv().await.unwrap().method != mouser_proto::control::Topic::Status {}
    })
    .await
    .expect("a status notification arrives");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ui_survives_the_engine_behind_the_pipe_restarting() {
    use mouser_proto::control::Method;
    use serde_json::json;
    let f = crate::support_tests::fixture().await;
    let path = crate::support_tests::control_path_in(&f._dir);
    let ui = remote::ui_host(&f).await;
    let mut notes = ui.control().notifications();
    // The UI was talking to it when...
    assert!(ui.control().call(Method::Status, None).await.is_ok());
    // ...a user switch: the service stops the engine...
    f.host.shutdown().await;
    remote::until_ready(&ui, false).await;
    let gone = ui.control().call(Method::Status, None).await.unwrap_err();
    assert_eq!(gone.message, mouser_core::control::client::GONE);
    let away = ui.control().call(Method::Status, None).await.unwrap_err();
    assert_eq!(away.message, crate::host::NOT_READY);
    // ...and starts it again on the same pipe.
    let dir = tempfile::tempdir().unwrap();
    let again = engine_at(&path, dir.path()).await;
    remote::until_ready(&ui, true).await;
    let status = ui.control().call(Method::Status, None).await.unwrap();
    let own = again.control().call(Method::Status, None).await.unwrap();
    assert_eq!(
        status["computer"]["id"], own["computer"]["id"],
        "the new engine"
    );
    // It hears every topic again.
    a_status_after(&mut notes, async {
        again
            .control()
            .call(Method::LoggingSet, Some(json!({"enabled": true})))
            .await
            .unwrap();
    })
    .await;
    again.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ui_started_before_the_engine_connects_once_it_is_there() {
    use mouser_proto::control::Method;
    let dir = tempfile::tempdir().unwrap();
    let path = crate::support_tests::control_path_in(&dir);
    // The service runs, its engine's pipe is not up yet: no crash.
    let ui = remote::ui_host_at(&path).await;
    assert!(!*ui.control().ready().borrow());
    let early = ui.control().call(Method::Status, None).await.unwrap_err();
    assert_eq!(early.message, crate::host::NOT_READY);
    let engine_dir = tempfile::tempdir().unwrap();
    let engine = engine_at(&path, engine_dir.path()).await;
    remote::until_ready(&ui, true).await;
    assert!(ui.control().call(Method::Status, None).await.is_ok());
    engine.shutdown().await;
}

#[test]
fn an_apps_own_engine_is_always_ready() {
    let (handle, ready) = tauri::async_runtime::block_on(async {
        let f = crate::support_tests::fixture().await;
        let ready = *f.host.control().ready().borrow();
        (f, ready)
    });
    drop(handle);
    assert!(ready);
}
