//! UI mode (research V9): on a Windows computer where Mouser is installed as
//! `MouserService`, the session engine runs the desk and the tray app runs
//! no engine of its own. It is a control-API client of the session engine
//! over `\.\pipe\mouser-service` (`Host::connect`).

use mouser_platform_windows::shell::{self, ServiceState};

/// The Windows service that runs the session engine.
pub const SERVICE_NAME: &str = "MouserService";

/// What the tray app asks of the service control manager.
pub trait ServiceQuery {
    fn service_state(&self) -> ServiceState;
    /// Starts the service: whether Windows took the request.
    fn start_service(&self) -> bool;
}

/// The real service control manager, about the service it names.
pub struct Scm(pub &'static str);

impl ServiceQuery for Scm {
    fn service_state(&self) -> ServiceState {
        shell::service_state(self.0)
    }

    fn start_service(&self) -> bool {
        shell::start_service(self.0)
    }
}

/// Whether the tray app starts in UI mode: while `MouserService` runs, or
/// once it has started it again after Quit stopped it (the installer lets
/// the desk user start it). The session engine is not there at once; the
/// tray says Mouser isn't ready until it is (T534). A service this user
/// cannot start leaves the tray app its own engine.
pub fn ui_mode(query: &dyn ServiceQuery) -> bool {
    match query.service_state() {
        ServiceState::Running => true,
        ServiceState::Stopped => query.start_service(),
        ServiceState::Missing => false,
    }
}
