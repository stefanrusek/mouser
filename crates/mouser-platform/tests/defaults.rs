//! The defaults of `Platform`: no problems reported, no foreground app known.

use mouser_platform::*;
use std::sync::mpsc::{Sender, channel};

struct Bare;

impl Platform for Bare {
    fn info(&self) -> PlatformInfo {
        unreachable!()
    }
    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        unreachable!()
    }
    fn watch_displays(&self, _tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        unreachable!()
    }
    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        unreachable!()
    }
    fn open_permission_settings(&self, _kind: PermissionKind) -> Result<()> {
        unreachable!()
    }
    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        unreachable!()
    }
    fn inject(&self) -> Result<Box<dyn InputInject>> {
        unreachable!()
    }
    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        unreachable!()
    }
    fn secure_input(&self) -> Option<SecureInputHolder> {
        unreachable!()
    }
    fn foreground_is_fullscreen(&self) -> bool {
        unreachable!()
    }
    fn interfaces(&self) -> Vec<NetInterface> {
        unreachable!()
    }
    fn watch_network(&self, _tx: Sender<()>) -> Result<WatchHandle> {
        unreachable!()
    }
    fn secrets(&self) -> Box<dyn SecretStore> {
        unreachable!()
    }
    fn autostart(&self) -> Box<dyn Autostart> {
        unreachable!()
    }
    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        unreachable!()
    }
}

#[test]
fn a_platform_reports_no_foreground_app_and_no_problems_by_default() {
    assert_eq!(Bare.foreground_app(), None);
    let (tx, rx) = channel();
    drop(Bare.watch_problems(tx).unwrap());
    assert!(rx.try_recv().is_err());
}
