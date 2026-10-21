use crate::echo::Echo;
use std::time::{Duration, Instant};

#[test]
fn only_events_near_the_recorded_position_inside_the_window_are_echoes() {
    let echo = Echo::default();
    let now = Instant::now();
    assert!(!echo.is_echo(20.0, 20.0, now));
    echo.record(20.0, 20.0);
    let soon = Instant::now();
    assert!(echo.is_echo(20.0, 20.0, soon));
    assert!(echo.is_echo(22.5, 17.5, soon));
    assert!(!echo.is_echo(24.0, 20.0, soon));
    assert!(!echo.is_echo(20.0, 24.0, soon));
    assert!(!echo.is_echo(20.0, 20.0, soon + Duration::from_secs(1)));
}

#[test]
fn a_newer_position_replaces_the_older() {
    let echo = Echo::default();
    echo.record(100.0, 100.0);
    echo.record(2.0, 2.0);
    let now = Instant::now();
    assert!(echo.is_echo(2.0, 2.0, now));
    assert!(!echo.is_echo(100.0, 100.0, now));
}
