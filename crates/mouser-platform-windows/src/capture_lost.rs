//! Capture loss (research V7; Story 1 sc. 3; platform rule 10). While the
//! capture is grabbed and can no longer see this computer's input, the
//! backend ungrabs and shows the pointer itself first, and only then tells
//! core, with `LocalInput::CaptureLost`. Nothing here waits for core, so the
//! pointer is back even when core is stuck. That happens when the app in
//! front outranks Mouser (checked each watchdog cycle and on each foreground
//! change), when another desktop has the input, or on one real miss of a
//! hook.

use crate::capture::Shared;
use mouser_platform::{CaptureLossReason, ForegroundApp, LocalInput};
use std::sync::atomic::Ordering;

/// Takes the capture away: when it is grabbed, ungrabs it, `show`s the
/// pointer and then `tell`s core why. A grab is lost once. Whether it was
/// grabbed.
pub(crate) fn lose(
    shared: &Shared,
    reason: CaptureLossReason,
    show: impl FnOnce(),
    tell: impl FnOnce(LocalInput),
) -> bool {
    if !shared.grabbed.swap(false, Ordering::SeqCst) {
        return false;
    }
    show();
    tell(LocalInput::CaptureLost(reason));
    true
}

/// Why the capture can no longer see input, from what a watchdog cycle
/// finds: another desktop has the input, or the app in front outranks
/// Mouser.
pub(crate) fn reason_now(
    foreground: Option<ForegroundApp>,
    other_desktop: bool,
) -> Option<CaptureLossReason> {
    if other_desktop {
        return Some(CaptureLossReason::OtherDesktop);
    }
    foreground
        .filter(|app| app.outranks_mouser)
        .map(|_| CaptureLossReason::ElevatedForeground)
}

/// The foreground changed: an app above Mouser takes the capture away at
/// once, without waiting for the next watchdog cycle.
pub(crate) fn on_foreground(app: Option<ForegroundApp>, lose: &dyn Fn(CaptureLossReason)) {
    #[cfg(test)]
    heard::tell(&app);
    if app.is_some_and(|app| app.outranks_mouser) {
        lose(CaptureLossReason::ElevatedForeground);
    }
}

/// Lets a test hear each foreground change `on_foreground` was given, as
/// (app name, outranks Mouser), so it can tell a change that was heard and
/// kept the capture from one that never reached the hook.
#[cfg(test)]
pub(crate) mod heard {
    use mouser_platform::ForegroundApp;
    use std::sync::Mutex;
    use std::sync::mpsc::Sender;

    static LISTENER: Mutex<Option<Sender<(String, bool)>>> = Mutex::new(None);

    /// Who hears the changes from now on; `None` for nobody.
    pub(crate) fn listen(tx: Option<Sender<(String, bool)>>) {
        *LISTENER.lock().unwrap() = tx;
    }

    pub(super) fn tell(app: &Option<ForegroundApp>) {
        if let (Some(tx), Some(app)) = (LISTENER.lock().unwrap().as_ref(), app) {
            let _ = tx.send((app.name.clone(), app.outranks_mouser));
        }
    }
}
