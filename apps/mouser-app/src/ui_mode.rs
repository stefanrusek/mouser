//! UI mode (research V9): on a Windows computer where Mouser is installed as
//! `MouserService`, the session engine runs the desk and the tray app runs
//! no engine of its own. It is a control-API client of the session engine
//! over `\\.\pipe\mouser-service` (`Host::connect`).

/// The Windows service that runs the session engine.
pub const SERVICE_NAME: &str = "MouserService";

/// Whether `MouserService` is installed and running.
pub trait ServiceQuery {
    fn service_running(&self) -> bool;
}

/// The real service control manager.
pub struct Scm;

impl ServiceQuery for Scm {
    fn service_running(&self) -> bool {
        mouser_platform_windows::shell::service_running(SERVICE_NAME)
    }
}

/// Whether the tray app starts in UI mode.
pub fn ui_mode(query: &dyn ServiceQuery) -> bool {
    query.service_running()
}
