//! Producer side of contracts/ui-navigation.md: `windows::navigate`.

use std::sync::{Arc, Mutex};

use tauri::{Listener, Manager};

use crate::support_tests::{app_with, fixture, has_window};
use crate::windows::{EVENT, LABEL, navigate};

#[tokio::test(flavor = "multi_thread")]
async fn navigate_creates_main_at_the_route_when_it_does_not_exist() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!has_window(&handle, LABEL));
    navigate(&handle, "#displays");
    let window = handle.get_webview_window(LABEL).expect("main is created");
    assert!(
        window
            .url()
            .unwrap()
            .as_str()
            .ends_with("index.html#displays")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn navigate_reuses_main_and_emits_the_route() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    navigate(&handle, "#desk");
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    handle
        .get_webview_window(LABEL)
        .unwrap()
        .listen(EVENT, move |event| {
            sink.lock().unwrap().push(event.payload().to_string())
        });
    navigate(&handle, "#health/ab12");
    crate::support_tests::eventually(|| !seen.lock().unwrap().is_empty()).await;
    assert_eq!(seen.lock().unwrap()[0], r##"{"route":"#health/ab12"}"##);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_call_ever_creates_a_second_normal_window() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    for route in ["#desk", "#displays", "#health", "#settings", "#welcome"] {
        navigate(&handle, route);
    }
    assert_eq!(handle.webview_windows().len(), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_the_dock_icon_is_regular_while_shown_and_accessory_while_hidden() {
    use crate::windows::activation_policy;
    use tauri::ActivationPolicy::{Accessory, Regular};
    assert!(matches!(activation_policy(true), Regular));
    assert!(matches!(activation_policy(false), Accessory));
}
