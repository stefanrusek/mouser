use mouser_platform::Os;
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
async fn a_second_launch_brings_up_the_first_ones_window() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!has_window(&handle, "main"));
    on_second_launch(&handle);
    eventually(|| has_window(&handle, "main")).await;
}
