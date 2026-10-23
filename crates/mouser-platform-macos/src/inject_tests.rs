use crate::capture::MOUSER_TAG;
use crate::inject::{MacInject, Poster};
use core_graphics::event::{CGEvent, CGEventType, EventField};
use mouser_platform::{Error, InputInject};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a posted event looked like, captured before it reaches the OS.
#[derive(Debug, Clone, PartialEq)]
struct Posted {
    ty: u32,
    tag: i64,
    keycode: i64,
    flags: u64,
    at: (f64, f64),
    button: i64,
    delta: (i64, i64),
    scroll: (i64, i64),
    clicks: i64,
}

/// A fake pointer and clock: tests move the pointer and advance time.
struct World {
    at: (f64, f64),
    start: Instant,
    elapsed: Duration,
}

#[derive(Clone)]
struct Recorder(Arc<Mutex<Vec<Posted>>>, Arc<Mutex<World>>);

impl Default for Recorder {
    fn default() -> Self {
        Recorder(
            Arc::default(),
            Arc::new(Mutex::new(World {
                at: (50.0, 60.0),
                start: Instant::now(),
                elapsed: Duration::ZERO,
            })),
        )
    }
}

impl Recorder {
    fn advance(&self, by: Duration) {
        self.1.lock().unwrap().elapsed += by;
    }
    fn point_at(&self, x: f64, y: f64) {
        self.1.lock().unwrap().at = (x, y);
    }
}

/// The double-click interval the fake world reports.
const INTERVAL: Duration = Duration::from_millis(400);

impl Poster for Recorder {
    fn post(&self, event: &CGEvent) {
        let at = event.location();
        self.0.lock().unwrap().push(Posted {
            ty: event.get_type() as u32,
            tag: event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA),
            keycode: event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE),
            flags: event.get_flags().bits(),
            at: (at.x, at.y),
            button: event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER),
            delta: (
                event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X),
                event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y),
            ),
            scroll: (
                event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2),
                event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1),
            ),
            clicks: event.get_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE),
        });
    }
    fn location(&self) -> (f64, f64) {
        self.1.lock().unwrap().at
    }
    fn now(&self) -> Instant {
        let world = self.1.lock().unwrap();
        world.start + world.elapsed
    }
    fn double_click_interval(&self) -> Duration {
        INTERVAL
    }
}

fn injector() -> (Recorder, MacInject) {
    let rec = Recorder::default();
    (rec.clone(), MacInject::new(Box::new(rec), Arc::default()))
}

fn take(rec: &Recorder) -> Vec<Posted> {
    std::mem::take(&mut *rec.0.lock().unwrap())
}

const SHIFT_FLAG: u64 = 0x0002_0000;
const CTRL_FLAG: u64 = 0x0004_0000;

#[test]
fn every_event_is_tagged_with_mouser_user_data() {
    let (rec, mut inj) = injector();
    inj.motion_abs(1.0, 2.0).unwrap();
    inj.button(1, true).unwrap();
    inj.button(1, false).unwrap();
    inj.scroll(0.0, 1.0).unwrap();
    inj.key(0x0007_0068, true).unwrap();
    inj.key(0x0007_0068, false).unwrap();
    let posted = take(&rec);
    assert_eq!(posted.len(), 6);
    assert!(posted.iter().all(|p| p.tag == MOUSER_TAG));
}

#[test]
fn motion_moves_the_pointer_with_deltas_from_the_previous_move() {
    let (rec, mut inj) = injector();
    inj.motion_abs(100.0, 200.0).unwrap();
    inj.motion_abs(110.0, 195.0).unwrap();
    let posted = take(&rec);
    assert_eq!(posted[0].ty, CGEventType::MouseMoved as u32);
    assert_eq!(posted[0].at, (100.0, 200.0));
    assert_eq!(posted[0].delta, (0, 0));
    assert_eq!(posted[1].at, (110.0, 195.0));
    assert_eq!(posted[1].delta, (10, -5));
}

#[test]
fn buttons_post_at_the_pointer_and_motion_while_held_is_a_drag() {
    let (rec, mut inj) = injector();
    inj.button(1, true).unwrap();
    inj.motion_abs(1.0, 1.0).unwrap();
    inj.button(1, false).unwrap();
    inj.button(2, true).unwrap();
    inj.motion_abs(2.0, 2.0).unwrap();
    inj.button(2, false).unwrap();
    inj.button(3, true).unwrap();
    inj.motion_abs(3.0, 3.0).unwrap();
    inj.button(3, false).unwrap();
    let types: Vec<u32> = take(&rec).iter().map(|p| p.ty).collect();
    assert_eq!(
        types,
        [
            CGEventType::LeftMouseDown as u32,
            CGEventType::LeftMouseDragged as u32,
            CGEventType::LeftMouseUp as u32,
            CGEventType::RightMouseDown as u32,
            CGEventType::RightMouseDragged as u32,
            CGEventType::RightMouseUp as u32,
            CGEventType::OtherMouseDown as u32,
            CGEventType::OtherMouseDragged as u32,
            CGEventType::OtherMouseUp as u32,
        ]
    );
}

#[test]
fn button_events_carry_position_and_cg_button_number() {
    let (rec, mut inj) = injector();
    inj.button(3, true).unwrap();
    inj.button(4, true).unwrap();
    let posted = take(&rec);
    assert_eq!(posted[0].at, (50.0, 60.0));
    assert_eq!(posted[0].button, 2);
    assert_eq!(posted[1].button, 3);
}

#[test]
fn scroll_posts_pixel_deltas() {
    let (rec, mut inj) = injector();
    inj.scroll(-3.0, 5.0).unwrap();
    let posted = take(&rec);
    assert_eq!(posted[0].ty, CGEventType::ScrollWheel as u32);
    assert_eq!(posted[0].scroll, (-3, 5));
}

#[test]
fn keys_post_the_keycode_for_their_hid_usage() {
    let (rec, mut inj) = injector();
    inj.key(0x0007_0004, true).unwrap(); // A
    inj.key(0x0007_0004, false).unwrap();
    inj.key(0x000C_00E9, true).unwrap(); // Volume Up
    let posted = take(&rec);
    assert_eq!(posted[0].ty, CGEventType::KeyDown as u32);
    assert_eq!(posted[0].keycode, 0x00);
    assert_eq!(posted[1].ty, CGEventType::KeyUp as u32);
    assert_eq!(posted[2].keycode, 0x48);
}

#[test]
fn keys_this_mac_lacks_are_dropped_not_posted() {
    let (rec, mut inj) = injector();
    inj.key(0x0007_0073, true).unwrap(); // F24
    inj.key(0x0007_0073, false).unwrap();
    assert!(take(&rec).is_empty());
}

#[test]
fn modifiers_post_flag_changes_and_apply_flags_to_later_keys() {
    let (rec, mut inj) = injector();
    inj.key(0x0007_00E1, true).unwrap(); // Left Shift down
    inj.key(0x0007_0004, true).unwrap(); // A with Shift held
    inj.key(0x0007_00E1, false).unwrap();
    inj.key(0x0007_0004, false).unwrap();
    let posted = take(&rec);
    assert_eq!(posted[0].ty, CGEventType::FlagsChanged as u32);
    assert_eq!(posted[0].flags & SHIFT_FLAG, SHIFT_FLAG);
    assert_eq!(posted[1].ty, CGEventType::KeyDown as u32);
    assert_eq!(posted[1].flags & SHIFT_FLAG, SHIFT_FLAG);
    assert_eq!(posted[2].ty, CGEventType::FlagsChanged as u32);
    assert_eq!(posted[2].flags & SHIFT_FLAG, 0);
    assert_eq!(posted[3].flags & SHIFT_FLAG, 0);
}

#[test]
fn dropping_releases_held_keys_and_buttons() {
    let (rec, mut inj) = injector();
    inj.key(0x0007_00E0, true).unwrap(); // Left Ctrl
    inj.key(0x0007_00E1, true).unwrap(); // Left Shift
    inj.key(0x0007_0004, true).unwrap();
    inj.button(2, true).unwrap();
    inj.button(1, true).unwrap();
    inj.button(1, false).unwrap();
    take(&rec);
    drop(inj);
    let posted = take(&rec);
    let ups: Vec<(u32, i64)> = posted.iter().map(|p| (p.ty, p.keycode)).collect();
    assert_eq!(
        &ups[..3],
        [
            (CGEventType::KeyUp as u32, 0x00),
            (CGEventType::FlagsChanged as u32, 0x3B),
            (CGEventType::FlagsChanged as u32, 0x38),
        ]
    );
    assert_eq!(posted[3].ty, CGEventType::RightMouseUp as u32);
    assert_eq!(posted.len(), 4);
    assert!(posted.iter().all(|p| p.tag == MOUSER_TAG));
    // The last modifier released leaves no modifier flag behind.
    assert_eq!(posted[2].flags & (SHIFT_FLAG | CTRL_FLAG), 0);
}

#[test]
fn secure_attention_is_unsupported() {
    let (_rec, mut inj) = injector();
    assert_eq!(inj.secure_attention(), Err(Error::Unsupported));
}

#[test]
fn the_system_poster_reports_a_pointer_location() {
    let (x, y) = crate::inject::HidPoster.location();
    assert!(x.is_finite() && y.is_finite());
}

#[test]
fn the_system_poster_reports_the_clock_and_the_users_double_click_interval() {
    let before = Instant::now();
    assert!(crate::inject::HidPoster.now() >= before);
    let interval = crate::inject::HidPoster.double_click_interval();
    assert!(interval > Duration::ZERO && interval <= Duration::from_secs(5));
}

/// The click count of every posted (type, clicks) pair.
fn clicks(rec: &Recorder) -> Vec<(u32, i64)> {
    take(rec).iter().map(|p| (p.ty, p.clicks)).collect()
}

fn click(inj: &mut MacInject, button: u8) {
    inj.button(button, true).unwrap();
    inj.button(button, false).unwrap();
}

const DOWN: u32 = CGEventType::LeftMouseDown as u32;
const UP: u32 = CGEventType::LeftMouseUp as u32;

#[test]
fn presses_in_quick_succession_count_up_single_double_triple_quadruple() {
    let (rec, mut inj) = injector();
    for _ in 0..4 {
        click(&mut inj, 1);
        rec.advance(Duration::from_millis(150));
    }
    assert_eq!(
        clicks(&rec),
        [
            (DOWN, 1),
            (UP, 1),
            (DOWN, 2),
            (UP, 2),
            (DOWN, 3),
            (UP, 3),
            (DOWN, 4),
            (UP, 4),
        ]
    );
}

#[test]
fn a_press_exactly_at_the_interval_still_counts_one_past_it_starts_over() {
    let (rec, mut inj) = injector();
    click(&mut inj, 1);
    rec.advance(INTERVAL);
    click(&mut inj, 1);
    rec.advance(INTERVAL + Duration::from_millis(1));
    click(&mut inj, 1);
    assert_eq!(
        clicks(&rec),
        [(DOWN, 1), (UP, 1), (DOWN, 2), (UP, 2), (DOWN, 1), (UP, 1)]
    );
}

#[test]
fn a_different_button_starts_a_new_count() {
    let (rec, mut inj) = injector();
    click(&mut inj, 1);
    click(&mut inj, 2);
    click(&mut inj, 1);
    click(&mut inj, 1);
    let right_down = CGEventType::RightMouseDown as u32;
    let right_up = CGEventType::RightMouseUp as u32;
    assert_eq!(
        clicks(&rec),
        [
            (DOWN, 1),
            (UP, 1),
            (right_down, 1),
            (right_up, 1),
            (DOWN, 1),
            (UP, 1),
            (DOWN, 2),
            (UP, 2),
        ]
    );
}

#[test]
fn a_press_more_than_a_few_points_away_starts_a_new_count() {
    let (rec, mut inj) = injector();
    click(&mut inj, 1);
    rec.point_at(53.0, 62.0); // within the slop of the first press
    click(&mut inj, 1);
    rec.point_at(60.0, 62.0); // well away from the second
    click(&mut inj, 1);
    assert_eq!(
        clicks(&rec),
        [(DOWN, 1), (UP, 1), (DOWN, 2), (UP, 2), (DOWN, 1), (UP, 1)]
    );
}

#[test]
fn a_drag_and_the_release_carry_the_count_of_their_press_and_moves_carry_none() {
    let (rec, mut inj) = injector();
    click(&mut inj, 1);
    inj.button(1, true).unwrap();
    // The press is a double click; a slow release keeps its count.
    rec.advance(INTERVAL * 3);
    inj.motion_abs(51.0, 60.0).unwrap();
    inj.button(1, false).unwrap();
    inj.motion_abs(52.0, 60.0).unwrap();
    // A release with no press is a single click.
    inj.button(1, false).unwrap();
    assert_eq!(
        clicks(&rec),
        [
            (DOWN, 1),
            (UP, 1),
            (DOWN, 2),
            (CGEventType::LeftMouseDragged as u32, 2),
            (UP, 2),
            (CGEventType::MouseMoved as u32, 0),
            (UP, 1),
        ]
    );
}

/// Posts nothing and reports a fixed location.
pub(crate) struct NullPoster;

impl Poster for NullPoster {
    fn post(&self, _event: &CGEvent) {}
    fn location(&self) -> (f64, f64) {
        (0.0, 0.0)
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn double_click_interval(&self) -> Duration {
        INTERVAL
    }
}

const CMD_FLAG: u64 = 0x0010_0000;
/// `NX_DEVICELCMDKEYMASK`: the left Command key, as the hardware reports it.
const LEFT_CMD_BIT: u64 = 0x0000_0008;
/// `kCGEventFlagMaskNonCoalesced`, set on every hardware key event.
const NON_COALESCED: u64 = 0x0000_0100;
const LEFT_META: u32 = 0x0007_00E3;
const C: u32 = 0x0007_0006;

#[test]
fn command_c_carries_the_flags_a_real_keyboard_reports_and_command_up_clears_them() {
    let (rec, mut inj) = injector();
    inj.key(LEFT_META, true).unwrap();
    inj.key(C, true).unwrap();
    inj.key(C, false).unwrap();
    inj.key(LEFT_META, false).unwrap();
    let posted: Vec<(u32, i64, u64)> = take(&rec)
        .iter()
        .map(|p| (p.ty, p.keycode, p.flags))
        .collect();
    let held = CMD_FLAG | LEFT_CMD_BIT | NON_COALESCED;
    assert_eq!(
        posted,
        [
            (CGEventType::FlagsChanged as u32, 0x37, held),
            (CGEventType::KeyDown as u32, 0x08, held),
            (CGEventType::KeyUp as u32, 0x08, held),
            (CGEventType::FlagsChanged as u32, 0x37, NON_COALESCED),
        ]
    );
}

#[test]
fn pointer_events_carry_exactly_the_injected_modifiers_not_the_system_state() {
    let (rec, mut inj) = injector();
    // Command held: a click is a Command-click, a move a Command-drag.
    inj.key(LEFT_META, true).unwrap();
    inj.button(1, true).unwrap();
    inj.motion_abs(5.0, 5.0).unwrap();
    inj.button(1, false).unwrap();
    inj.scroll(0.0, 1.0).unwrap();
    // Command released: nothing is left behind on what follows, whatever
    // the system's own modifier state says.
    inj.key(LEFT_META, false).unwrap();
    inj.button(1, true).unwrap();
    inj.motion_abs(6.0, 6.0).unwrap();
    inj.button(1, false).unwrap();
    inj.scroll(0.0, 1.0).unwrap();
    let flags: Vec<u64> = take(&rec)
        .iter()
        .filter(|p| p.ty != CGEventType::FlagsChanged as u32)
        .map(|p| p.flags)
        .collect();
    let cmd = CMD_FLAG | LEFT_CMD_BIT;
    assert_eq!(flags, [cmd, cmd, cmd, cmd, 0, 0, 0, 0]);
}
