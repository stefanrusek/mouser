use crate::autostart::{LoginItem, Service, SystemService};
use mouser_platform::{Autostart, Error};
use smappservice_rs::ServiceStatus;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Fake {
    status: Arc<Mutex<ServiceStatus>>,
    fail: bool,
}

impl Service for Fake {
    fn status(&self) -> ServiceStatus {
        *self.status.lock().unwrap()
    }
    fn register(&self) -> Result<(), String> {
        if self.fail {
            return Err("no bundle".into());
        }
        *self.status.lock().unwrap() = ServiceStatus::Enabled;
        Ok(())
    }
    fn unregister(&self) -> Result<(), String> {
        if self.fail {
            return Err("not registered".into());
        }
        *self.status.lock().unwrap() = ServiceStatus::NotRegistered;
        Ok(())
    }
}

fn fake(status: ServiceStatus, fail: bool) -> Fake {
    Fake {
        status: Arc::new(Mutex::new(status)),
        fail,
    }
}

#[test]
fn set_registers_and_unregisters_and_enabled_reflects_it() {
    let mut item = LoginItem::new(fake(ServiceStatus::NotRegistered, false));
    assert!(!item.enabled());
    item.set(true).unwrap();
    assert!(item.enabled());
    item.set(false).unwrap();
    assert!(!item.enabled());
}

#[test]
fn approval_pending_is_not_enabled() {
    assert!(!LoginItem::new(fake(ServiceStatus::RequiresApproval, false)).enabled());
}

#[test]
fn service_failures_surface_as_os_errors() {
    let mut item = LoginItem::new(fake(ServiceStatus::NotRegistered, true));
    assert!(matches!(item.set(true), Err(Error::Os(m)) if m.contains("no bundle")));
    assert!(matches!(item.set(false), Err(Error::Os(m)) if m.contains("not registered")));
}

/// Registering the running test binary would leave a Background Items record
/// behind, so the real SMAppService calls are driven against a login-item
/// identifier that does not exist. The main-app path needs the signed bundle.
#[test]
fn real_service_refuses_an_unknown_login_item() {
    let mut item = LoginItem::new(SystemService(Some("org.mouser.no-such-helper")));
    assert!(!item.enabled());
    assert!(matches!(item.set(true), Err(Error::Os(m)) if m.starts_with("login item: ")));
    assert!(matches!(item.set(false), Err(Error::Os(_))));
    assert!(!item.enabled());
}

#[test]
fn main_app_service_is_not_enabled_for_a_bare_binary() {
    assert!(!LoginItem::new(SystemService(None)).enabled());
}
