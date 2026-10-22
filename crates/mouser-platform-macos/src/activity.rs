//! Opt out of App Nap while Mouser captures input.
//!
//! macOS throttles and coalesces timers of processes it considers idle (App
//! Nap), and a throttled process services its `CGEventTap` late enough for the
//! system to disable the tap by timeout. Holding an activity token for as long
//! as capture runs prevents that.
//!
//! Options: `UserInitiatedAllowingIdleSystemSleep` is the minimal set that
//! disables App Nap (and sudden/automatic termination) without keeping the
//! Mac awake: the computer may still sleep when the user is idle.
//! `LatencyCritical` additionally turns off timer coalescing, so the tap
//! callback and the input threads are scheduled promptly; a late callback is
//! exactly what makes the OS drop the tap.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};

pub(crate) const OPTIONS: NSActivityOptions = NSActivityOptions(
    NSActivityOptions::UserInitiatedAllowingIdleSystemSleep.0
        | NSActivityOptions::LatencyCritical.0,
);

/// An App Nap opt-out that ends when dropped.
pub(crate) struct Activity {
    token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl Activity {
    pub(crate) fn begin(reason: &str) -> Self {
        let token = NSProcessInfo::processInfo()
            .beginActivityWithOptions_reason(OPTIONS, &NSString::from_str(reason));
        Activity { token }
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        // SAFETY: the token came from beginActivityWithOptions:reason: and is
        // ended exactly once, here.
        unsafe { NSProcessInfo::processInfo().endActivity(&self.token) };
    }
}

// SAFETY: the token is an opaque object only ever passed back to the
// thread-safe `NSProcessInfo`; it is never messaged otherwise.
unsafe impl Send for Activity {}
