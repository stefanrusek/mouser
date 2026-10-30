use crate::host::Host;
use crate::host_tests::remote::ui_host;
use crate::support_tests::fixture;
use crate::ui_mode::*;

struct Answers(bool);

impl ServiceQuery for Answers {
    fn service_running(&self) -> bool {
        self.0
    }
}

#[test]
fn the_tray_app_runs_in_ui_mode_only_while_mouser_service_runs() {
    assert!(ui_mode(&Answers(true)));
    assert!(!ui_mode(&Answers(false)));
}

#[test]
fn the_real_query_asks_windows_about_mouser_service() {
    assert_eq!(
        Scm.service_running(),
        mouser_platform_windows::shell::service_running(SERVICE_NAME)
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
