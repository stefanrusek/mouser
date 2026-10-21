//! A computer whose clipboard holds only some kinds of content: everything
//! else is the wrapped virtual computer.

use std::sync::mpsc::Sender;

use mouser_platform::{
    Autostart, ClipRep, Clipboard, Error, FileDrag, InputCapture, InputInject, LocalDisplay,
    NetInterface, PermissionKind, PermissionState, Platform, PlatformInfo, Result, SecretStore,
    SecureInputHolder, WatchHandle,
};
use mouser_testkit::VirtualPlatform;

pub struct Picky {
    pub inner: VirtualPlatform,
    pub holds: fn(&ClipRep) -> bool,
}

struct PickyClipboard {
    inner: Box<dyn Clipboard>,
    holds: fn(&ClipRep) -> bool,
}

impl Clipboard for PickyClipboard {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        self.inner.read()
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        if reps.iter().all(self.holds) {
            self.inner.write(reps)
        } else {
            Err(Error::Unsupported)
        }
    }

    fn change_count(&mut self) -> u64 {
        self.inner.change_count()
    }
}

impl Platform for Picky {
    fn info(&self) -> PlatformInfo {
        self.inner.info()
    }

    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        self.inner.displays()
    }

    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        self.inner.watch_displays(tx)
    }

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        self.inner.permissions()
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        self.inner.open_permission_settings(kind)
    }

    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        self.inner.capture()
    }

    fn inject(&self) -> Result<Box<dyn InputInject>> {
        self.inner.inject()
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        Ok(Box::new(PickyClipboard {
            inner: self.inner.clipboard()?,
            holds: self.holds,
        }))
    }

    fn secure_input(&self) -> Option<SecureInputHolder> {
        self.inner.secure_input()
    }

    fn foreground_is_fullscreen(&self) -> bool {
        self.inner.foreground_is_fullscreen()
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        self.inner.interfaces()
    }

    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle> {
        self.inner.watch_network(tx)
    }

    fn foreground_elevated(&self) -> bool {
        self.inner.foreground_elevated()
    }

    fn secrets(&self) -> Box<dyn SecretStore> {
        self.inner.secrets()
    }

    fn autostart(&self) -> Box<dyn Autostart> {
        self.inner.autostart()
    }

    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        self.inner.file_drag()
    }
}
