use super::echo::Echo;
use std::time::{Duration, Instant};

#[test]
fn motion_at_the_injected_position_is_an_echo_for_half_a_second() {
    let echo = Echo::default();
    let t0 = Instant::now();
    assert!(!echo.is_motion((20, 20), t0));
    echo.record_motion(20, 20, t0);
    assert!(echo.is_motion((20, 20), t0 + Duration::from_millis(100)));
    assert!(echo.is_motion((20, 20), t0 + Duration::from_millis(100)));
    assert!(!echo.is_motion((21, 20), t0 + Duration::from_millis(100)));
    assert!(!echo.is_motion((20, 20), t0 + Duration::from_millis(600)));
}

#[test]
fn each_recorded_button_event_is_consumed_once() {
    let echo = Echo::default();
    let t0 = Instant::now();
    echo.record_button(1, true, t0);
    echo.record_button(1, false, t0);
    assert!(!echo.take_button(2, true, t0));
    assert!(echo.take_button(1, false, t0));
    assert!(!echo.take_button(1, false, t0));
    assert!(echo.take_button(1, true, t0));
}

#[test]
fn each_recorded_key_event_is_consumed_once() {
    let echo = Echo::default();
    let t0 = Instant::now();
    echo.record_key(38, true, t0);
    assert!(!echo.take_key(38, false, t0));
    assert!(echo.take_key(38, true, t0));
    assert!(!echo.take_key(38, true, t0));
}

#[test]
fn stale_records_are_forgotten() {
    let echo = Echo::default();
    let t0 = Instant::now();
    echo.record_key(38, true, t0);
    echo.record_button(1, true, t0);
    let later = t0 + Duration::from_secs(1);
    assert!(!echo.take_key(38, true, later));
    assert!(!echo.take_button(1, true, later));
}
