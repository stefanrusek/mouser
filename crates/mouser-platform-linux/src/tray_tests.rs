use crate::tray::{NO_CLICK, NO_TRAY, limitations};
use mouser_platform::Capability;

#[test]
fn without_a_status_notifier_watcher_the_appindicator_extension_is_named_under_tray() {
    let l = limitations(false);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].capability, Capability::Tray);
    assert_eq!(l[0].text, NO_TRAY);
    assert!(l[0].text.contains("AppIndicator"));
}

#[test]
fn a_tray_that_emits_no_click_events_lists_the_pause_limitation() {
    let l = limitations(true);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].capability, Capability::Tray);
    assert_eq!(
        l[0].text,
        "Clicking the tray icon doesn't pause Mouser here; use Pause in its menu."
    );
    assert_eq!(l[0].text, NO_CLICK);
}
