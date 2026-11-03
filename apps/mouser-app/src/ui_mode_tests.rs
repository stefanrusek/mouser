use std::cell::Cell;

use mouser_platform_windows::shell::ServiceState;

use crate::host::Host;
use crate::host_tests::remote::ui_host;
use crate::support_tests::fixture;
use crate::ui_mode::*;

/// A service control manager that answers `state`, and starts the service
/// when `starts` says so.
struct Answers {
    state: ServiceState,
    starts: bool,
    started: Cell<bool>,
}

fn answers(state: ServiceState, starts: bool) -> Answers {
    Answers {
        state,
        starts,
        started: Cell::new(false),
    }
}

impl ServiceQuery for Answers {
    fn service_state(&self) -> ServiceState {
        self.state
    }

    fn start_service(&self) -> bool {
        self.started.set(true);
        self.starts
    }
}

#[test]
fn the_tray_app_runs_in_ui_mode_while_mouser_service_runs() {
    let scm = answers(ServiceState::Running, false);
    assert!(ui_mode(&scm));
    assert!(!scm.started.get());
}

#[test]
fn without_mouser_service_the_tray_app_runs_its_own_engine() {
    let scm = answers(ServiceState::Missing, true);
    assert!(!ui_mode(&scm));
    assert!(!scm.started.get());
}

#[test]
fn a_stopped_mouser_service_is_started_and_the_tray_app_then_runs_in_ui_mode() {
    // Quit stopped it; the desk user started Mouser again.
    let scm = answers(ServiceState::Stopped, true);
    assert!(ui_mode(&scm));
    assert!(scm.started.get());
}

#[test]
fn a_stopped_mouser_service_that_will_not_start_leaves_the_tray_app_its_own_engine() {
    // An install from before the desk user could start it.
    let scm = answers(ServiceState::Stopped, false);
    assert!(!ui_mode(&scm));
    assert!(scm.started.get());
}

#[test]
fn the_real_query_asks_windows_about_mouser_service() {
    assert_eq!(
        Scm(SERVICE_NAME).service_state(),
        mouser_platform_windows::shell::service_state(SERVICE_NAME)
    );
    assert_eq!(SERVICE_NAME, "MouserService");
}

#[tokio::test(flavor = "multi_thread")]
async fn collecting_logs_in_ui_mode_goes_through_the_remote_link() {
    use tauri::Manager;
    let f = fixture().await;
    let ui = ui_host(&f).await;
    let app = crate::support_tests::app_with(ui);
    // The engine behind it is not the service, so its own bundle comes back.
    let result = crate::commands::collect_logs(app.state::<Host>())
        .await
        .unwrap();
    assert!(result.get("path").is_some(), "{result}");
}

#[test]
fn the_real_start_asks_windows_to_start_the_service() {
    // A service that is not there is not started.
    assert!(!Scm("NoSuchMouserServiceForTests").start_service());
}
