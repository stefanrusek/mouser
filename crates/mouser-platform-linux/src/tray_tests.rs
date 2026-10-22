use crate::tray::{NO_TRAY, limitations};

#[test]
fn without_a_status_notifier_watcher_the_appindicator_extension_is_named() {
    let l = limitations(false);
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].text, NO_TRAY);
    assert!(l[0].text.contains("AppIndicator"));
}

#[test]
fn with_a_watcher_there_is_no_limitation() {
    assert!(limitations(true).is_empty());
}
