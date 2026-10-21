use crate::fullscreen::{
    Probe, Rect, SystemProbe, Window, foreground_is_fullscreen, is_fullscreen,
};

const DISPLAY: Rect = Rect {
    x: 0.0,
    y: 0.0,
    w: 1512.0,
    h: 982.0,
};
const SHIELD: i32 = 2_147_483_628;

fn win(pid: u32, layer: i32, bounds: Rect) -> Window {
    Window { pid, layer, bounds }
}

fn check(windows: &[Window]) -> bool {
    is_fullscreen(42, DISPLAY, windows, SHIELD)
}

#[test]
fn frontmost_app_window_filling_the_display_is_fullscreen() {
    assert!(check(&[win(42, 0, DISPLAY)]));
}

#[test]
fn bounds_within_one_point_still_match() {
    let near = Rect {
        x: 1.0,
        y: -1.0,
        w: 1511.0,
        h: 983.0,
    };
    assert!(check(&[win(42, 0, near)]));
    let off = Rect { x: 1.5, ..DISPLAY };
    assert!(!check(&[win(42, 0, off)]));
    let short = Rect {
        h: 980.0,
        ..DISPLAY
    };
    assert!(!check(&[win(42, 0, short)]));
    let narrow = Rect {
        w: 1510.0,
        ..DISPLAY
    };
    assert!(!check(&[win(42, 0, narrow)]));
    let low = Rect { y: 1.5, ..DISPLAY };
    assert!(!check(&[win(42, 0, low)]));
}

#[test]
fn a_maximised_but_not_fullscreen_window_is_not() {
    let below_menu_bar = Rect {
        x: 0.0,
        y: 38.0,
        w: 1512.0,
        h: 944.0,
    };
    assert!(!check(&[win(42, 0, below_menu_bar)]));
    assert!(!check(&[]));
}

#[test]
fn only_the_first_layer_zero_window_of_the_frontmost_app_on_the_display_counts() {
    let small = Rect {
        x: 100.0,
        y: 100.0,
        w: 400.0,
        h: 300.0,
    };
    // The frontmost app's top window is small; a full-size one is behind it.
    assert!(!check(&[win(42, 0, small), win(42, 0, DISPLAY)]));
    // Other apps' full-size windows and non-zero layers are skipped.
    assert!(check(&[
        win(7, 0, DISPLAY),
        win(42, 25, small),
        win(42, 0, DISPLAY)
    ]));
    assert!(!check(&[win(7, 0, DISPLAY), win(42, 25, DISPLAY)]));
}

#[test]
fn windows_on_other_displays_are_skipped() {
    let elsewhere = Rect {
        x: 1512.0,
        y: 0.0,
        w: 1512.0,
        h: 982.0,
    };
    assert!(!check(&[win(42, 0, elsewhere)]));
    assert!(check(&[win(42, 0, elsewhere), win(42, 0, DISPLAY)]));
}

#[test]
fn any_shielding_level_window_covering_the_display_is_fullscreen() {
    assert!(check(&[win(7, SHIELD, DISPLAY)]));
    assert!(check(&[win(7, SHIELD + 1, DISPLAY)]));
    let small = Rect {
        w: 100.0,
        ..DISPLAY
    };
    assert!(!check(&[win(7, SHIELD, small)]));
    assert!(!check(&[win(7, SHIELD - 1, DISPLAY)]));
}

struct Fake(bool);

impl Probe for Fake {
    fn frontmost_pid(&self) -> u32 {
        42
    }
    fn cursor_display(&self) -> Rect {
        DISPLAY
    }
    fn windows(&self) -> Vec<Window> {
        if self.0 {
            vec![win(42, 0, DISPLAY)]
        } else {
            vec![]
        }
    }
    fn shielding_level(&self) -> i32 {
        SHIELD
    }
}

#[test]
fn the_probe_feeds_the_decision() {
    assert!(foreground_is_fullscreen(&Fake(true)));
    assert!(!foreground_is_fullscreen(&Fake(false)));
}

#[test]
fn system_probe_reads_the_live_desktop() {
    // Read-only queries: window list, cursor position, frontmost app.
    let probe = SystemProbe;
    assert!(probe.frontmost_pid() > 0);
    let display = probe.cursor_display();
    assert!(display.w > 0.0 && display.h > 0.0);
    assert!(probe.shielding_level() > 1000);
    let windows = probe.windows();
    assert!(windows.iter().all(|w| w.bounds.w >= 0.0));
    // Whatever is on screen, the full decision must complete.
    let _ = foreground_is_fullscreen(&probe);
}
