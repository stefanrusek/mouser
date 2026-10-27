//! Consumer side of the platform seam: every `mouser_platform::conformance`
//! check run against `VirtualPlatform`, both fully capable and `limited()`.

use mouser_platform::Platform;
use mouser_platform::conformance::*;
use mouser_testkit::VirtualPlatform;

fn on_both(check: fn(&dyn Platform)) {
    check(&VirtualPlatform::new());
    check(&VirtualPlatform::limited());
}

#[test]
fn contract_rule_1_grab() {
    on_both(rule_1_grab);
}

/// A platform that cannot force a grab (as on Wayland) still passes rule 1.
#[test]
fn contract_rule_1_grab_when_grabbing_is_unsupported() {
    rule_1_grab(&NoGrab(VirtualPlatform::new()));
}

/// `VirtualPlatform`, except that its capture refuses to grab.
struct NoGrab(pub VirtualPlatform);

struct NoGrabCapture(Box<dyn mouser_platform::InputCapture>);

impl mouser_platform::InputCapture for NoGrabCapture {
    fn start(
        &mut self,
        tx: std::sync::mpsc::Sender<mouser_platform::LocalInput>,
    ) -> mouser_platform::Result<()> {
        self.0.start(tx)
    }

    fn set_grabbed(&mut self, _grabbed: bool) -> mouser_platform::Result<()> {
        Err(mouser_platform::Error::Unsupported)
    }

    fn warp(&mut self, x: f64, y: f64) -> mouser_platform::Result<()> {
        self.0.warp(x, y)
    }

    fn stop(&mut self) {
        self.0.stop()
    }
}

impl mouser_platform::Platform for NoGrab {
    fn info(&self) -> mouser_platform::PlatformInfo {
        self.0.info()
    }

    fn displays(&self) -> mouser_platform::Result<Vec<mouser_platform::LocalDisplay>> {
        self.0.displays()
    }

    fn watch_displays(
        &self,
        tx: std::sync::mpsc::Sender<Vec<mouser_platform::LocalDisplay>>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.0.watch_displays(tx)
    }

    fn permissions(
        &self,
    ) -> Vec<(
        mouser_platform::PermissionKind,
        mouser_platform::PermissionState,
    )> {
        self.0.permissions()
    }

    fn open_permission_settings(
        &self,
        kind: mouser_platform::PermissionKind,
    ) -> mouser_platform::Result<()> {
        self.0.open_permission_settings(kind)
    }

    fn capture(&self) -> mouser_platform::Result<Box<dyn mouser_platform::InputCapture>> {
        Ok(Box::new(NoGrabCapture(self.0.capture()?)))
    }

    fn inject(&self) -> mouser_platform::Result<Box<dyn mouser_platform::InputInject>> {
        self.0.inject()
    }

    fn clipboard(&self) -> mouser_platform::Result<Box<dyn mouser_platform::Clipboard>> {
        self.0.clipboard()
    }

    fn secure_input(&self) -> Option<mouser_platform::SecureInputHolder> {
        self.0.secure_input()
    }

    fn foreground_is_fullscreen(&self) -> bool {
        self.0.foreground_is_fullscreen()
    }

    fn interfaces(&self) -> Vec<mouser_platform::NetInterface> {
        self.0.interfaces()
    }

    fn watch_network(
        &self,
        tx: std::sync::mpsc::Sender<()>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.0.watch_network(tx)
    }

    fn watch_problems(
        &self,
        tx: std::sync::mpsc::Sender<mouser_platform::PlatformProblem>,
    ) -> mouser_platform::Result<mouser_platform::WatchHandle> {
        self.0.watch_problems(tx)
    }

    fn foreground_app(&self) -> Option<mouser_platform::ForegroundApp> {
        self.0.foreground_app()
    }

    fn secrets(&self) -> Box<dyn mouser_platform::SecretStore> {
        self.0.secrets()
    }

    fn autostart(&self) -> Box<dyn mouser_platform::Autostart> {
        self.0.autostart()
    }

    fn file_drag(&self) -> Option<Box<dyn mouser_platform::FileDrag>> {
        self.0.file_drag()
    }
}

#[test]
fn contract_rule_2_release() {
    on_both(rule_2_release);
}

#[test]
fn contract_rule_3_hid_usage() {
    on_both(rule_3_hid_usage);
}

#[test]
fn contract_rule_4_displays() {
    on_both(rule_4_displays);
}

#[test]
fn contract_rule_5_clipboard() {
    on_both(rule_5_clipboard);
}

#[test]
fn contract_rule_6_interfaces() {
    on_both(rule_6_interfaces);
}

#[test]
fn contract_rule_7_no_self_capture() {
    on_both(rule_7_no_self_capture);
}

#[test]
fn contract_rule_8_secrets() {
    on_both(rule_8_secrets);
}

#[test]
fn contract_rule_9_limitations() {
    on_both(rule_9_limitations);
}

#[test]
fn contract_all_checks_listed() {
    assert_eq!(ALL.len(), 9);
    for (_, check) in ALL {
        on_both(*check);
    }
}

#[test]
fn contract_rule_10_pointer_visible() {
    let full = VirtualPlatform::new();
    rule_10_pointer_visible(&full, &|| full.pointer_hidden());
    let limited = VirtualPlatform::limited();
    rule_10_pointer_visible(&limited, &|| limited.pointer_hidden());
    let wayland = NoGrab(VirtualPlatform::new());
    rule_10_pointer_visible(&wayland, &|| wayland.0.pointer_hidden());
}

#[test]
fn contract_rule_11_refusal_is_error() {
    on_both(rule_11_refusal_is_error);
}
