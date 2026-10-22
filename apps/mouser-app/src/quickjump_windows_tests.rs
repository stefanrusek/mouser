use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;

use std::sync::{Arc, Mutex};

use crate::quickjump_windows::{EVENT, LABEL_PREFIX, Plan, hide, plan, show};
use crate::support_tests::{app_with, eventually, fixture};

fn letter_windows(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> Vec<String> {
    use tauri::Manager;
    let mut labels: Vec<String> = app
        .webview_windows()
        .into_keys()
        .filter(|label| label.starts_with(LABEL_PREFIX))
        .collect();
    labels.sort();
    labels
}

/// Count the "QuickJump ended" events each letter window receives. (The mock
/// runtime has no event loop, so a destroyed window is observed by being told.)
fn count_endings(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> Arc<Mutex<usize>> {
    use tauri::{Listener, Manager};
    let seen = Arc::new(Mutex::new(0));
    for (label, window) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let sink = seen.clone();
            window.listen(EVENT, move |_| *sink.lock().unwrap() += 1);
        }
    }
    seen
}

fn local(key: &str, origin: (f64, f64)) -> LocalDisplay {
    LocalDisplay {
        key: key.into(),
        name: key.into(),
        pixel_size: (2000, 1000),
        logical_size: (1000.0, 500.0),
        scale: 2.0,
        physical_size_mm: None,
        origin,
    }
}

fn id(computer: u8, key: &str) -> DisplayId {
    DisplayId {
        computer: ComputerId([computer; 16]),
        key: key.into(),
    }
}

#[test]
fn only_this_computers_displays_get_a_window_at_their_own_place() {
    let displays = [local("a", (0.0, 0.0)), local("b", (1000.0, -40.0))];
    let letters = [
        (id(1, "a"), 'A'),
        (id(2, "a"), 'B'),
        (id(1, "b"), 'C'),
        (id(1, "gone"), 'D'),
    ];
    let planned = plan(&ComputerId([1; 16]), &displays, &letters);
    assert_eq!(
        planned,
        vec![
            Plan {
                label: format!("{LABEL_PREFIX}0"),
                letter: 'A',
                origin: (0.0, 0.0),
                size: (1000.0, 500.0),
            },
            Plan {
                label: format!("{LABEL_PREFIX}1"),
                letter: 'C',
                origin: (1000.0, -40.0),
                size: (1000.0, 500.0),
            },
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn showing_creates_one_window_per_display_and_hiding_removes_them() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let plans = plan(
        &ComputerId([1; 16]),
        &[local("a", (0.0, 0.0)), local("b", (1000.0, 0.0))],
        &[(id(1, "a"), 'A'), (id(1, "b"), 'B')],
    );
    show(&handle, &plans);
    assert_eq!(letter_windows(&handle).len(), 2);
    let ended = count_endings(&handle);
    hide(&handle);
    eventually(|| *ended.lock().unwrap() == 2).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn showing_again_replaces_the_letters_on_screen() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let two = plan(
        &ComputerId([1; 16]),
        &[local("a", (0.0, 0.0)), local("b", (1000.0, 0.0))],
        &[(id(1, "a"), 'A'), (id(1, "b"), 'B')],
    );
    show(&handle, &two);
    let ended = count_endings(&handle);
    let one = plan(
        &ComputerId([1; 16]),
        &[local("a", (0.0, 0.0))],
        &[(id(1, "a"), 'C')],
    );
    show(&handle, &one);
    eventually(|| *ended.lock().unwrap() == 2).await;
    assert_eq!(letter_windows(&handle).len(), 3);
}
