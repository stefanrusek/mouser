use std::sync::{Arc, Mutex};

use tauri::{Listener, Manager};

use crate::overlay::{EVENT, LABEL, SIZE, create, show_landing};
use crate::support_tests::{app_with, eventually, fixture, has_window};

#[tokio::test(flavor = "multi_thread")]
async fn the_overlay_window_is_created_at_startup() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    assert!(!has_window(&handle, LABEL));
    create(&handle);
    assert!(has_window(&handle, LABEL));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_landing_tells_the_overlay_where_to_draw_the_highlight() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    create(&handle);
    let window = handle.get_webview_window(LABEL).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    window.listen(EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    show_landing(&handle, (1000.0, 500.0));
    eventually(|| seen.lock().unwrap().len() == 1).await;
    let half = SIZE / 2.0;
    assert_eq!(
        seen.lock().unwrap()[0],
        format!("{{\"x\":{half:?},\"y\":{half:?}}}")
    );
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "the landing overlay was created at startup")]
async fn a_landing_before_the_overlay_exists_is_a_bug() {
    let f = fixture().await;
    let app = app_with(f.host);
    show_landing(app.handle(), (0.0, 0.0));
}
