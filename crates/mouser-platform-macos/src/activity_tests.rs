use crate::activity::{Activity, OPTIONS};
use objc2_foundation::NSActivityOptions;

#[test]
fn options_disable_app_nap_without_keeping_the_system_awake() {
    assert_eq!(
        OPTIONS.0 & NSActivityOptions::UserInitiatedAllowingIdleSystemSleep.0,
        NSActivityOptions::UserInitiatedAllowingIdleSystemSleep.0
    );
    assert_ne!(OPTIONS.0 & NSActivityOptions::LatencyCritical.0, 0);
    assert_eq!(OPTIONS.0 & NSActivityOptions::IdleSystemSleepDisabled.0, 0);
}

#[test]
fn an_activity_begins_and_ends_with_the_os() {
    let activity = Activity::begin("Mouser test");
    drop(activity);
}
