use crate::fullscreen::*;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Shell::{
    QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE,
    QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_POPUP};

const MONITOR: RECT = RECT {
    left: 0,
    top: 0,
    right: 2560,
    bottom: 1440,
};

fn window(rect: RECT, class: &str, style: u32) -> Foreground {
    Foreground {
        window: rect,
        monitor: MONITOR,
        class: class.into(),
        style,
    }
}

#[test]
fn busy_and_direct3d_full_screen_states_mean_full_screen() {
    assert!(state_means_fullscreen(QUNS_BUSY));
    assert!(state_means_fullscreen(QUNS_RUNNING_D3D_FULL_SCREEN));
}

#[test]
fn other_notification_states_do_not() {
    for state in [
        QUNS_NOT_PRESENT,
        QUNS_PRESENTATION_MODE,
        QUNS_ACCEPTS_NOTIFICATIONS,
        QUNS_QUIET_TIME,
        QUNS_APP,
    ] {
        assert!(!state_means_fullscreen(state), "{state:?}");
    }
}

#[test]
fn a_borderless_window_covering_its_monitor_is_full_screen() {
    assert!(window_is_fullscreen(&window(MONITOR, "Game", WS_POPUP.0)));
    let bigger = RECT {
        left: -1,
        top: -1,
        right: 2561,
        bottom: 1441,
    };
    assert!(window_is_fullscreen(&window(bigger, "Game", WS_POPUP.0)));
}

#[test]
fn a_window_short_of_any_edge_is_not() {
    let short = [
        RECT { left: 1, ..MONITOR },
        RECT { top: 1, ..MONITOR },
        RECT {
            right: 2559,
            ..MONITOR
        },
        RECT {
            bottom: 1439,
            ..MONITOR
        },
    ];
    for r in short {
        assert!(!window_is_fullscreen(&window(r, "Game", WS_POPUP.0)));
    }
}

#[test]
fn a_maximized_window_with_a_title_bar_is_not() {
    let f = window(MONITOR, "Notepad", WS_CAPTION.0);
    assert!(!window_is_fullscreen(&f));
}

#[test]
fn the_desktop_and_taskbar_never_count() {
    for class in [
        "Progman",
        "WorkerW",
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
    ] {
        assert!(!window_is_fullscreen(&window(MONITOR, class, WS_POPUP.0)));
    }
}

#[test]
fn either_signal_is_enough() {
    let game = window(MONITOR, "Game", WS_POPUP.0);
    let small = window(RECT::default(), "Game", WS_POPUP.0);
    assert!(decide(Some(QUNS_BUSY), None));
    assert!(decide(None, Some(&game)));
    assert!(decide(Some(QUNS_NOT_PRESENT), Some(&game)));
    assert!(!decide(Some(QUNS_NOT_PRESENT), Some(&small)));
    assert!(!decide(None, None));
}

#[test]
fn the_live_check_answers_without_panicking() {
    let _ = foreground_is_fullscreen();
}

#[test]
fn the_foreground_window_is_described_when_there_is_one() {
    if let Some(f) = foreground() {
        assert!(f.monitor.right > f.monitor.left);
        assert!(!f.class.is_empty());
    }
}
