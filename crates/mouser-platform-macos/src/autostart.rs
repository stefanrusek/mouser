//! Login-item autostart through `SMAppService` (macOS 13+). It registers the
//! running main app, which only works from a signed `.app` bundle.

use mouser_platform::{Autostart, Error, Result};
use smappservice_rs::{AppService, ServiceStatus, ServiceType};

/// The two SMAppService operations used, as a seam.
pub(crate) trait Service: Send {
    fn status(&self) -> ServiceStatus;
    fn register(&self) -> std::result::Result<(), String>;
    fn unregister(&self) -> std::result::Result<(), String>;
}

/// `None` is the running main app (production); `Some(id)` addresses a login
/// item helper by bundle identifier, which lets tests drive the real
/// SMAppService calls against an identifier that does not exist.
pub(crate) struct SystemService(pub(crate) Option<&'static str>);

impl SystemService {
    fn app_service(&self) -> AppService {
        AppService::new(match self.0 {
            None => ServiceType::MainApp,
            Some(identifier) => ServiceType::LoginItem { identifier },
        })
    }
}

impl Service for SystemService {
    fn status(&self) -> ServiceStatus {
        self.app_service().status()
    }
    fn register(&self) -> std::result::Result<(), String> {
        self.app_service().register().map_err(|e| e.to_string())
    }
    fn unregister(&self) -> std::result::Result<(), String> {
        self.app_service().unregister().map_err(|e| e.to_string())
    }
}

pub(crate) struct LoginItem<S: Service> {
    service: S,
}

impl<S: Service> LoginItem<S> {
    pub(crate) fn new(service: S) -> Self {
        LoginItem { service }
    }
}

impl<S: Service> Autostart for LoginItem<S> {
    fn enabled(&self) -> bool {
        self.service.status() == ServiceStatus::Enabled
    }

    fn set(&mut self, on: bool) -> Result<()> {
        let result = if on {
            self.service.register()
        } else {
            self.service.unregister()
        };
        result.map_err(|e| Error::Os(format!("login item: {e}")))
    }
}
