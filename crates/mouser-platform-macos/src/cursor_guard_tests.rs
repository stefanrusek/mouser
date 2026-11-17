use crate::cursor::Cursor;
use crate::cursor_guard::{
    COALESCE, Correction, Deviation, Expectation, Guard, Observation, Phase, REASSOCIATE, SETTLE,
    TICK, Watcher, decide, deviation,
};
use crate::diag::heard_during;
use crate::echo::Echo;
use mouser_platform::{DiagLevel, DiagValue, Error, Result};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PARK: (f64, f64) = (300.0, 200.0);

struct State {
    pos: (f64, f64),
    /// Outstanding hides, from Mouser and from other apps; visible at zero.
    depth: i32,
    associated: bool,
    calls: Vec<String>,
    /// `(call, how many more times it is refused)`.
    refusals: Vec<(&'static str, u32)>,
}

/// A cursor with the state the OS keeps, which "other apps" can change.
pub(crate) struct Scripted(Mutex<State>);

impl Scripted {
    /// A cursor as a grab leaves it: parked at `PARK`, hidden once, disassociated.
    pub(crate) fn grabbed() -> Arc<Self> {
        Arc::new(Scripted(Mutex::new(State {
            pos: PARK,
            depth: 1,
            associated: false,
            calls: Vec::new(),
            refusals: Vec::new(),
        })))
    }
    /// A cursor nobody has touched: visible, associated, at `PARK`.
    pub(crate) fn untouched() -> Arc<Self> {
        let cursor = Self::grabbed();
        cursor.0.lock().unwrap().depth = 0;
        cursor.0.lock().unwrap().associated = true;
        cursor
    }
    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.0.lock().unwrap())
    }
    fn attempt(&self, call: &'static str, text: String) -> Result<()> {
        self.with(|s| {
            s.calls.push(text);
            match s.refusals.iter_mut().find(|(c, n)| *c == call && *n > 0) {
                Some((_, left)) => {
                    *left -= 1;
                    Err(Error::Os(format!("{call} refused")))
                }
                None => Ok(()),
            }
        })
    }
    pub(crate) fn refuse(&self, call: &'static str, times: u32) {
        self.with(|s| s.refusals.push((call, times)));
    }
    pub(crate) fn take_calls(&self) -> Vec<String> {
        self.with(|s| std::mem::take(&mut s.calls))
    }
    pub(crate) fn is_visible(&self) -> bool {
        self.with(|s| s.depth <= 0)
    }
    pub(crate) fn depth(&self) -> i32 {
        self.with(|s| s.depth)
    }
    pub(crate) fn is_associated(&self) -> bool {
        self.with(|s| s.associated)
    }
    pub(crate) fn at(&self) -> (f64, f64) {
        self.with(|s| s.pos)
    }
    /// Another app hides the cursor (a capture).
    pub(crate) fn foreign_hide(&self) {
        self.with(|s| s.depth += 1);
    }
    /// Another app releases its capture: the cursor is visible and free.
    pub(crate) fn foreign_release(&self) {
        self.with(|s| {
            s.depth = 0;
            s.associated = true;
        });
    }
    pub(crate) fn foreign_warp(&self, x: f64, y: f64) {
        self.with(|s| s.pos = (x, y));
    }
    pub(crate) fn foreign_disassociate(&self) {
        self.with(|s| s.associated = false);
    }
}

impl Cursor for Scripted {
    fn position(&self) -> (f64, f64) {
        self.with(|s| s.pos)
    }
    fn warp(&self, x: f64, y: f64) -> Result<()> {
        self.attempt("warp", format!("warp {x} {y}"))?;
        self.with(|s| s.pos = (x, y));
        Ok(())
    }
    fn associate(&self, on: bool) -> Result<()> {
        self.attempt("associate", format!("associate {on}"))?;
        self.with(|s| s.associated = on);
        Ok(())
    }
    fn hide(&self) -> Result<()> {
        self.attempt("hide", "hide".into())?;
        self.with(|s| s.depth += 1);
        Ok(())
    }
    fn show(&self) -> Result<()> {
        self.attempt("show", "show".into())?;
        self.with(|s| s.depth = (s.depth - 1).max(0));
        Ok(())
    }
    fn visible(&self) -> bool {
        self.is_visible()
    }
}

fn remote(hides: u32) -> Expectation {
    Expectation {
        phase: Phase::Remote,
        park: PARK,
        hides,
        arrived_at: Instant::now(),
    }
}

fn seen(visible: bool, position: (f64, f64)) -> Observation {
    Observation {
        visible,
        position,
        at: Instant::now(),
    }
}

#[test]
fn decide_has_nothing_to_correct_when_the_cursor_is_as_expected() {
    assert!(decide(&remote(1), &seen(false, PARK), false).is_empty());
    // Within a point of the park is not a move.
    assert!(decide(&remote(1), &seen(false, (301.0, 199.0)), false).is_empty());
}

#[test]
fn decide_hides_a_cursor_that_became_visible_showing_what_it_hid_first() {
    assert_eq!(
        decide(&remote(1), &seen(true, PARK), false),
        [
            Correction::Show,
            Correction::Hide,
            Correction::Associate(false)
        ]
    );
    assert_eq!(
        decide(&remote(0), &seen(true, PARK), false),
        [Correction::Hide, Correction::Associate(false)]
    );
}

#[test]
fn decide_parks_a_cursor_that_moved_unless_the_move_is_mousers_own() {
    let moved = seen(false, (400.0, 200.0));
    assert_eq!(
        decide(&remote(1), &moved, false),
        [Correction::Warp(300.0, 200.0), Correction::Associate(false)]
    );
    assert!(decide(&remote(1), &seen(false, (300.0, 202.0)), false).len() == 2);
    // FR-004: Mouser's own warp is not a foreign change.
    assert!(decide(&remote(1), &moved, true).is_empty());
}

#[test]
fn decide_restores_visibility_before_position() {
    assert_eq!(
        decide(&remote(1), &seen(true, (0.0, 0.0)), false),
        [
            Correction::Show,
            Correction::Hide,
            Correction::Warp(300.0, 200.0),
            Correction::Associate(false)
        ]
    );
    assert_eq!(
        deviation(&remote(1), &seen(true, (0.0, 0.0)), false),
        Deviation {
            visible: true,
            position: true
        }
    );
}

#[test]
fn decide_enforces_nothing_when_idle() {
    let idle = Expectation {
        phase: Phase::Idle,
        ..remote(0)
    };
    assert!(decide(&idle, &seen(true, (0.0, 0.0)), false).is_empty());
}

struct Rig {
    cursor: Arc<Scripted>,
    echo: Arc<Echo>,
    guard: Guard,
    start: Instant,
}

impl Rig {
    fn new() -> Rig {
        let cursor = Scripted::grabbed();
        let echo = Arc::new(Echo::default());
        let mut guard = Guard::new(cursor.clone(), echo.clone());
        let start = Instant::now();
        guard.begin_remote(PARK, start);
        Rig {
            cursor,
            echo,
            guard,
            start,
        }
    }
    fn tick_at(&mut self, ms: u64) {
        self.guard.tick(self.start + Duration::from_millis(ms));
    }
    fn assert_held(&self) {
        assert!(!self.cursor.is_visible());
        assert_eq!(self.cursor.depth(), 1);
        assert_eq!(self.cursor.at(), PARK);
        assert!(!self.cursor.is_associated());
        assert_eq!(self.guard.expectation().hides, 1);
    }
}

#[test]
fn an_idle_guard_reads_and_corrects_nothing() {
    let cursor = Scripted::untouched();
    let mut guard = Guard::new(cursor.clone(), Arc::default());
    guard.tick(Instant::now());
    assert!(cursor.take_calls().is_empty());
    assert_eq!(guard.end(), 0);
}

#[test]
fn a_capture_then_release_then_repeated_recapture_is_corrected_every_time() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.tick_at(20);
        assert!(rig.cursor.take_calls().is_empty(), "nothing to correct");
        // A game captures: hides again and warps to its window centre.
        rig.cursor.foreign_hide();
        rig.cursor.foreign_warp(960.0, 540.0);
        rig.tick_at(40);
        assert_eq!(rig.cursor.at(), PARK);
        // The game releases: cursor visible and associated.
        rig.cursor.foreign_release();
        rig.tick_at(60);
        rig.assert_held();
        for i in 0..5 {
            rig.cursor.foreign_release();
            rig.cursor.foreign_warp(10.0, 10.0);
            rig.tick_at(80 + 20 * i);
            rig.assert_held();
        }
    });
    assert!(!heard.is_empty());
}

#[test]
fn the_s5_sequence_visible_flips_with_the_position_unchanged_is_corrected() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_release();
        rig.cursor.foreign_warp(PARK.0, PARK.1);
        rig.tick_at(20);
        rig.assert_held();
        assert_eq!(
            rig.cursor.take_calls(),
            ["show", "hide", "associate false"],
            "visibility first, no warp"
        );
    });
    assert_eq!(heard.len(), 2, "visible and associated: {heard:?}");
}

#[test]
fn the_s1_sequence_position_jumps_to_the_centre_and_is_restored() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_warp(960.0, 540.0);
        rig.tick_at(20);
        rig.assert_held();
        assert_eq!(rig.cursor.take_calls(), ["warp 300 200", "associate false"]);
    });
    assert_eq!(heard.len(), 2, "position and associated: {heard:?}");
}

#[test]
fn mousers_own_warp_is_not_read_as_foreign_on_the_next_tick() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_warp(960.0, 540.0);
        rig.tick_at(20);
        rig.cursor.take_calls();
        // The cursor sits where Mouser warped it: the echo covers it even if
        // the park point were different; nothing further happens.
        rig.echo.record(PARK.0, PARK.1);
        rig.tick_at(40);
        assert!(rig.cursor.take_calls().is_empty());
    });
    let counts: Vec<_> = heard.iter().map(|l| l.2[3].1.clone()).collect();
    assert_eq!(
        counts,
        [DiagValue::Int(1), DiagValue::Int(1)],
        "one change only"
    );
}

#[test]
fn an_echoed_position_deviation_is_not_corrected_or_logged() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_warp(500.0, 500.0);
        rig.echo.record(500.0, 500.0);
        rig.tick_at(20);
        assert!(rig.cursor.take_calls().is_empty());
        rig.guard.end();
    });
    assert!(heard.is_empty());
}

#[test]
fn a_refused_show_is_never_followed_by_a_hide_and_is_retried() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_release();
        rig.cursor.refuse("show", 1);
        rig.tick_at(20);
        assert_eq!(rig.cursor.take_calls(), ["show"]);
        assert!(rig.cursor.is_visible());
        assert_eq!(rig.guard.expectation().hides, 1, "count not unbalanced");
        rig.tick_at(40);
        rig.assert_held();
    });
    // A refused correction is recorded as not corrected, the retry as corrected.
    let corrected: Vec<_> = heard.iter().map(|l| l.2[2].1.clone()).collect();
    assert!(corrected.contains(&DiagValue::Bool(false)));
}

#[test]
fn a_refused_hide_leaves_the_count_balanced_and_is_retried() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_release();
        rig.cursor.refuse("hide", 1);
        rig.tick_at(20);
        assert_eq!(rig.guard.expectation().hides, 0);
        assert_eq!(rig.cursor.depth(), 0);
        rig.tick_at(40);
        rig.assert_held();
    });
    assert!(!heard.is_empty());
}

#[test]
fn association_is_reasserted_every_interval_and_not_before() {
    let mut rig = Rig::new();
    let at = REASSOCIATE.as_millis() as u64;
    rig.tick_at(at - 20);
    assert!(rig.cursor.take_calls().is_empty());
    rig.tick_at(at);
    assert_eq!(rig.cursor.take_calls(), ["associate false"]);
    rig.tick_at(at + 20);
    assert!(rig.cursor.take_calls().is_empty());
    rig.tick_at(2 * at);
    assert_eq!(rig.cursor.take_calls(), ["associate false"]);
}

#[test]
fn a_deviation_that_already_reasserts_association_does_not_do_it_twice() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_warp(0.0, 0.0);
        rig.tick_at(REASSOCIATE.as_millis() as u64);
        assert_eq!(rig.cursor.take_calls(), ["warp 300 200", "associate false"]);
    });
    assert!(!heard.is_empty());
}

#[test]
fn a_space_switch_that_resets_the_cursor_is_restored_on_the_next_tick() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        // The OS resets everything under the guard.
        rig.cursor.foreign_release();
        rig.cursor.foreign_warp(0.0, 0.0);
        rig.tick_at(20);
        rig.assert_held();
    });
    assert!(!heard.is_empty());
}

#[test]
fn two_capturing_apps_end_held_and_the_log_is_coalesced_per_aspect() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        // Past one coalescing window: the first record closes at 1 s.
        let ticks = COALESCE.as_millis() as u64 / TICK.as_millis() as u64 + 1;
        for i in 1..=ticks {
            // App B shows and warps; app A hides again.
            rig.cursor.foreign_release();
            rig.cursor.foreign_warp(960.0, 540.0);
            rig.tick_at(20 * i);
            rig.cursor.foreign_hide();
            rig.tick_at(20 * i);
        }
        // App A's hide is still on top of Mouser's.
        assert!(!rig.cursor.is_visible());
        assert_eq!(rig.cursor.at(), PARK);
        assert!(!rig.cursor.is_associated());
    });
    assert_eq!(heard.len(), 3, "one record per aspect: {heard:?}");
    let count = i64::try_from(COALESCE.as_millis() / TICK.as_millis() + 1).unwrap();
    for (line, aspect) in heard.iter().zip(["visible", "associated", "position"]) {
        assert_eq!(line.0, DiagLevel::Warn);
        assert_eq!(line.1, "cursor.foreign_change");
        assert_eq!(
            line.2,
            [
                ("aspect", DiagValue::Str(aspect.into())),
                ("phase", DiagValue::Str("remote".into())),
                ("corrected", DiagValue::Bool(true)),
                ("count", DiagValue::Int(count)),
            ]
        );
    }
}

#[test]
fn counted_deviations_are_written_when_the_guard_ends() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_release();
        rig.tick_at(20);
        assert_eq!(rig.guard.end(), 1, "the hide Mouser still owes a show");
        assert_eq!(rig.guard.end(), 0);
    });
    assert_eq!(heard.len(), 2);
}

#[test]
fn a_new_grab_starts_with_a_clean_count() {
    let heard = heard_during(|| {
        let mut rig = Rig::new();
        rig.cursor.foreign_release();
        rig.tick_at(20);
        rig.guard
            .begin_remote(PARK, rig.start + Duration::from_secs(5));
    });
    assert_eq!(heard.len(), 2, "flushed at the new grab, once");
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn the_watcher_corrects_a_deviation_on_its_own_and_stops_when_dropped() {
    let heard = heard_during(|| {
        let cursor = Scripted::grabbed();
        let mut guard = Guard::new(cursor.clone(), Arc::default());
        guard.begin_remote(PARK, Instant::now());
        let guard = Arc::new(Mutex::new(guard));
        let watcher = Watcher::spawn(guard.clone());
        cursor.foreign_release();
        wait_until("the correction", || !cursor.is_visible());
        drop(watcher);
        // Stopped: a later deviation is left alone.
        cursor.foreign_release();
        std::thread::sleep(TICK * 4);
        assert!(cursor.is_visible());
        guard.lock().unwrap().end();
    });
    assert!(!heard.is_empty());
}

#[test]
fn dropping_a_watcher_right_after_spawning_joins_promptly() {
    let guard = Arc::new(Mutex::new(Guard::new(
        Scripted::untouched(),
        Arc::default(),
    )));
    let started = Instant::now();
    drop(Watcher::spawn(guard));
    assert!(started.elapsed() < Duration::from_secs(2));
}

fn arrived() -> Expectation {
    Expectation {
        phase: Phase::Arrived,
        hides: 0,
        ..remote(0)
    }
}

#[test]
fn decide_shows_and_associates_a_cursor_hidden_while_arrived() {
    assert_eq!(
        decide(&arrived(), &seen(false, PARK), false),
        vec![Correction::Show, Correction::Associate(true)]
    );
}

#[test]
fn decide_leaves_a_visible_arrived_cursor_and_its_position_alone() {
    assert!(decide(&arrived(), &seen(true, PARK), false).is_empty());
    assert!(decide(&arrived(), &seen(true, (5.0, 5.0)), false).is_empty());
}

/// A rig that has arrived at `PARK` at `start`, cursor shown and associated.
fn arrived_rig() -> Rig {
    let mut rig = Rig::new();
    rig.cursor.foreign_release();
    let hides = rig.guard.arrive(PARK, rig.start);
    assert_eq!(hides, 1);
    rig.cursor.take_calls();
    rig
}

#[test]
fn a_game_that_rehides_within_the_settle_window_is_corrected() {
    let heard = heard_during(|| {
        let mut rig = arrived_rig();
        rig.cursor.foreign_hide();
        rig.cursor.foreign_disassociate();
        assert!(rig.guard.tick(rig.start + Duration::from_millis(100)));
        assert!(rig.cursor.is_visible());
        assert!(rig.cursor.is_associated());
    });
    assert!(!heard.is_empty());
}

#[test]
fn a_game_that_rehides_after_the_settle_window_is_not_corrected() {
    let heard = heard_during(|| {
        let mut rig = arrived_rig();
        rig.tick_at(500);
        rig.cursor.take_calls();
        rig.cursor.foreign_hide();
        assert!(!rig.guard.tick(rig.start + Duration::from_millis(1200)));
        assert!(!rig.cursor.is_visible());
        assert!(rig.cursor.take_calls().is_empty());
        assert_eq!(rig.guard.expectation().phase, Phase::Idle);
    });
    assert!(heard.is_empty());
}

#[test]
fn the_guard_yields_at_exactly_the_settle_boundary() {
    let mut rig = arrived_rig();
    rig.cursor.foreign_hide();
    assert!(
        rig.guard
            .tick(rig.start + SETTLE - Duration::from_millis(1))
    );
    rig.cursor.foreign_hide();
    assert!(!rig.guard.tick(rig.start + SETTLE));
    assert!(!rig.cursor.is_visible(), "no correction at the boundary");
    assert_eq!(rig.guard.expectation().phase, Phase::Idle);
}

#[test]
fn arrived_reasserts_association_as_true_on_the_slow_beat() {
    let mut rig = arrived_rig();
    rig.cursor.foreign_disassociate();
    rig.tick_at(REASSOCIATE.as_millis() as u64);
    assert!(rig.cursor.is_associated());
}

#[test]
fn crossing_back_and_forth_quickly_ends_up_in_the_latest_phase() {
    let heard = heard_during(|| {
        let mut rig = arrived_rig();
        // Back to the other computer within the window.
        rig.guard
            .begin_remote(PARK, rig.start + Duration::from_millis(50));
        rig.cursor.foreign_hide();
        rig.cursor.foreign_disassociate();
        rig.guard.tick(rig.start + Duration::from_millis(70));
        assert_eq!(rig.guard.expectation().phase, Phase::Remote);
        rig.cursor.foreign_release();
        rig.guard.tick(rig.start + Duration::from_millis(90));
        assert!(!rig.cursor.is_visible());
        assert!(!rig.cursor.is_associated());
        // And onto the Mac again: a fresh window starts from this arrival.
        let hides = rig
            .guard
            .arrive(PARK, rig.start + Duration::from_millis(900));
        for _ in 0..hides {
            rig.cursor.show().unwrap();
        }
        rig.cursor.associate(true).unwrap();
        assert!(rig.guard.tick(rig.start + Duration::from_millis(1500)));
        assert!(!rig.guard.tick(rig.start + Duration::from_millis(1900)));
    });
    assert!(!heard.is_empty());
}
