//! The cursor guard: while Mouser holds the pointer for the other computer,
//! it keeps the Mac cursor hidden, still and disassociated, however often
//! another app (a game that captures the mouse) undoes that.
//!
//! No notification exists for cursor visibility or association changes, so a
//! thread reads visibility and position every [`TICK`] and corrects any
//! deviation through the [`Cursor`] seam. Association cannot be read; it is
//! re-asserted every [`REASSOCIATE`] and whenever something else deviated.
//! The guard takes no lock the tap callback takes (FR-010).
//!
//! The decision is a pure function ([`decide`]); [`Guard::tick`] applies it;
//! [`Watcher`] is the few lines of thread around `tick`.

use crate::cursor::Cursor;
use crate::diag;
use crate::echo::Echo;
use mouser_platform::{DiagLevel, DiagValue};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often the guard reads the cursor.
pub(crate) const TICK: Duration = Duration::from_millis(20);
/// How often association is re-asserted when nothing else deviated.
pub(crate) const REASSOCIATE: Duration = Duration::from_millis(250);
/// How long deviations are counted before one log record per aspect is written.
pub(crate) const COALESCE: Duration = Duration::from_secs(1);
/// How long after the pointer arrives the guard holds the cursor shown and
/// associated against an app that captures again, before that app wins.
pub(crate) const SETTLE: Duration = Duration::from_millis(1000);

/// How far (points, per axis) the cursor may be from the park point.
const PARK_TOLERANCE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Grabbed: hidden, still, disassociated.
    Remote,
    /// Ungrabbed, within [`SETTLE`]: visible and associated.
    Arrived,
    /// Nothing to enforce.
    Idle,
}

/// What Mouser wants the Mac cursor to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Expectation {
    pub(crate) phase: Phase,
    /// Where the cursor was parked when the grab began.
    pub(crate) park: (f64, f64),
    /// Hides Mouser has issued and not yet shown.
    pub(crate) hides: u32,
    /// When `Arrived` began.
    pub(crate) arrived_at: Instant,
}

/// What the guard read this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Observation {
    pub(crate) visible: bool,
    pub(crate) position: (f64, f64),
    pub(crate) at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Correction {
    Show,
    Hide,
    Associate(bool),
    Warp(f64, f64),
}

/// Which of the observable aspects deviate from the expectation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Deviation {
    pub(crate) visible: bool,
    pub(crate) position: bool,
}

pub(crate) fn deviation(exp: &Expectation, obs: &Observation, own_echo: bool) -> Deviation {
    match exp.phase {
        Phase::Idle | Phase::Arrived => Deviation {
            visible: false,
            position: false,
        },
        Phase::Remote => Deviation {
            visible: obs.visible,
            position: !own_echo
                && ((obs.position.0 - exp.park.0).abs() > PARK_TOLERANCE
                    || (obs.position.1 - exp.park.1).abs() > PARK_TOLERANCE),
        },
    }
}

/// The corrections that restore the expectation, in the order to apply them.
/// `own_echo` is true when the observed position is one Mouser itself set.
pub(crate) fn decide(exp: &Expectation, obs: &Observation, own_echo: bool) -> Vec<Correction> {
    if exp.phase == Phase::Arrived {
        return if obs.visible {
            Vec::new()
        } else {
            vec![Correction::Show, Correction::Associate(true)]
        };
    }
    let dev = deviation(exp, obs, own_echo);
    let mut out = Vec::new();
    if dev.visible {
        out.extend((0..exp.hides).map(|_| Correction::Show));
        out.push(Correction::Hide);
    }
    if dev.position {
        out.push(Correction::Warp(exp.park.0, exp.park.1));
    }
    if dev.visible || dev.position {
        out.push(Correction::Associate(false));
    }
    out
}

const VISIBLE: usize = 0;
const ASSOCIATED: usize = 1;
const POSITION: usize = 2;
const ASPECTS: [&str; 3] = ["visible", "associated", "position"];

#[derive(Clone, Copy)]
struct Tally {
    count: i64,
    corrected: bool,
}

pub(crate) struct Guard {
    cursor: Arc<dyn Cursor>,
    echo: Arc<Echo>,
    exp: Expectation,
    last_associated: Instant,
    window_start: Instant,
    tallies: [Option<Tally>; 3],
}

impl Guard {
    pub(crate) fn new(cursor: Arc<dyn Cursor>, echo: Arc<Echo>) -> Self {
        let now = Instant::now();
        Guard {
            cursor,
            echo,
            exp: Expectation {
                phase: Phase::Idle,
                park: (0.0, 0.0),
                hides: 0,
                arrived_at: now,
            },
            last_associated: now,
            window_start: now,
            tallies: [None; 3],
        }
    }

    /// The grab began: the cursor is parked at `park` and Mouser hid it once.
    pub(crate) fn begin_remote(&mut self, park: (f64, f64), now: Instant) {
        self.flush();
        self.exp = Expectation {
            phase: Phase::Remote,
            park,
            hides: 1,
            arrived_at: now,
        };
        self.last_associated = now;
    }

    /// The pointer arrived: the cursor was taken back at `landing` and is
    /// held shown and associated until [`SETTLE`] has passed. Returns the
    /// hides still to show.
    pub(crate) fn arrive(&mut self, landing: (f64, f64), now: Instant) -> u32 {
        self.flush();
        let hides = std::mem::take(&mut self.exp.hides);
        self.exp = Expectation {
            phase: Phase::Arrived,
            park: landing,
            hides: 0,
            arrived_at: now,
        };
        self.last_associated = now;
        hides
    }

    /// The grab ended: enforce nothing more. Returns the hides still to show.
    pub(crate) fn end(&mut self) -> u32 {
        self.flush();
        self.exp.phase = Phase::Idle;
        std::mem::take(&mut self.exp.hides)
    }

    #[cfg(test)]
    pub(crate) fn expectation(&self) -> Expectation {
        self.exp
    }

    /// Reads the cursor and restores the expectation if it deviates.
    /// Returns false once there is nothing left to enforce.
    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        if self.exp.phase == Phase::Arrived
            && now.saturating_duration_since(self.exp.arrived_at) >= SETTLE
        {
            self.flush();
            self.exp.phase = Phase::Idle;
        }
        if self.exp.phase == Phase::Idle {
            return false;
        }
        let obs = Observation {
            visible: self.cursor.visible(),
            position: self.cursor.position(),
            at: now,
        };
        let own_echo = self.echo.is_echo(obs.position.0, obs.position.1, now);
        let mut dev = deviation(&self.exp, &obs, own_echo);
        dev.visible |= self.exp.phase == Phase::Arrived && !obs.visible;
        let mut corrections = decide(&self.exp, &obs, own_echo);
        let reassociate_due = obs.at.saturating_duration_since(self.last_associated) >= REASSOCIATE;
        let held = self.exp.phase != Phase::Remote;
        if reassociate_due && !corrections.contains(&Correction::Associate(held)) {
            corrections.push(Correction::Associate(held));
        }
        let corrected = self.apply(&corrections, now);
        if dev.visible || dev.position {
            self.record(dev, corrected, now);
        }
        if now.saturating_duration_since(self.window_start) >= COALESCE {
            self.flush();
        }
        true
    }

    /// Applies corrections in order. A refused call is logged and the rest
    /// are left for the next tick (which recomputes them from what it reads),
    /// so a refused show is never followed by a hide.
    fn apply(&mut self, corrections: &[Correction], now: Instant) -> bool {
        for correction in corrections {
            let result = match *correction {
                Correction::Show => self
                    .cursor
                    .show()
                    .map(|()| self.exp.hides = self.exp.hides.saturating_sub(1)),
                Correction::Hide => self.cursor.hide().map(|()| self.exp.hides += 1),
                Correction::Associate(on) => self
                    .cursor
                    .associate(on)
                    .map(|()| self.last_associated = now),
                Correction::Warp(x, y) => {
                    self.echo.record(x, y);
                    self.cursor.warp(x, y)
                }
            };
            if let Err(refused) = result {
                eprintln!("mouser: cursor guard correction refused, retrying: {refused}");
                return false;
            }
        }
        true
    }

    /// Counts a deviation per aspect. `associated` is inferred: a re-assert
    /// follows every visible or position deviation.
    fn record(&mut self, dev: Deviation, corrected: bool, now: Instant) {
        if self.tallies.iter().all(Option::is_none) {
            self.window_start = now;
        }
        let aspects = [
            (VISIBLE, dev.visible),
            (ASSOCIATED, true),
            (POSITION, dev.position),
        ];
        for aspect in aspects.into_iter().filter_map(|(a, hit)| hit.then_some(a)) {
            let tally = self.tallies[aspect].get_or_insert(Tally {
                count: 0,
                corrected: true,
            });
            tally.count += 1;
            tally.corrected &= corrected;
        }
    }

    /// Writes one `cursor.foreign_change` record per counted aspect.
    fn flush(&mut self) {
        let phase = if self.exp.phase == Phase::Arrived {
            "arrived"
        } else {
            "remote"
        };
        for (aspect, tally) in self.tallies.iter_mut().enumerate() {
            if let Some(tally) = tally.take() {
                diag::emit(
                    DiagLevel::Warn,
                    "cursor.foreign_change",
                    vec![
                        ("aspect", DiagValue::Str(ASPECTS[aspect].into())),
                        ("phase", DiagValue::Str(phase.into())),
                        ("corrected", DiagValue::Bool(tally.corrected)),
                        ("count", DiagValue::Int(tally.count)),
                    ],
                );
            }
        }
    }
}

impl Drop for Guard {
    /// Counted deviations are never lost to a guard that goes away.
    fn drop(&mut self) {
        self.flush();
    }
}

/// The guard thread: ticks until stopped. Dropping it stops and joins.
pub(crate) struct Watcher {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    pub(crate) fn spawn(guard: Arc<Mutex<Guard>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            loop {
                std::thread::park_timeout(TICK);
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                if !guard.lock().unwrap().tick(Instant::now()) {
                    break;
                }
            }
        });
        Watcher {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let thread = self.thread.take().unwrap();
        thread.thread().unpark();
        thread.join().unwrap();
    }
}
