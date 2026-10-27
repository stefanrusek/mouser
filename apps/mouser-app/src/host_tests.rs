use mouser_platform::{Os, Platform};
use mouser_proto::control::Method;

use crate::host::on_second_launch;
use crate::support_tests::{app_with, eventually, fixture, has_window, status_of};

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

#[tokio::test(flavor = "multi_thread")]
async fn a_second_launch_brings_up_the_first_ones_window() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!has_window(&handle, "main"));
    on_second_launch(&handle);
    eventually(|| has_window(&handle, "main")).await;
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
