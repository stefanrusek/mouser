//! Transfer progress (Story 7 scenario 2): the file being copied, how much
//! is done, the speed and the time remaining, reported as `transfer`
//! notifications.

use std::time::{Duration, Instant};

use mouser_proto::wire::TransferId;
use serde_json::{Value, json};

use super::engine::State;
use super::streams::hex;

/// Progress is reported at most this often while bytes are flowing.
pub const REPORT_EVERY: Duration = Duration::from_millis(100);

/// How far a transfer has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub current_item: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Bytes per second since the first byte arrived.
    pub rate: u64,
    /// Seconds until done at that rate; unknown before there is a rate.
    pub eta_seconds: Option<u64>,
}

#[derive(Debug)]
pub struct Progress {
    total: u64,
    done: u64,
    current: String,
    started: Option<Instant>,
    reported: Option<Instant>,
}

impl Progress {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            done: 0,
            current: String::new(),
            started: None,
            reported: None,
        }
    }

    /// The transfer reached `name`.
    pub fn begin_item(&mut self, name: &str) {
        self.current = name.to_string();
    }

    /// `bytes` more arrived.
    pub fn add(&mut self, bytes: u64, now: Instant) {
        self.started.get_or_insert(now);
        self.done += bytes;
    }

    /// A file of `size` bytes will not be copied.
    pub fn skip(&mut self, size: u64) {
        self.total -= size;
    }

    /// Whether enough time has passed to report again; if so, now counts as
    /// the last report.
    pub fn due(&mut self, now: Instant) -> bool {
        let due = self
            .reported
            .is_none_or(|last| now.duration_since(last) >= REPORT_EVERY);
        if due {
            self.reported = Some(now);
        }
        due
    }

    pub fn snapshot(&self, now: Instant) -> Snapshot {
        let elapsed = self
            .started
            .map_or(0.0, |s| now.duration_since(s).as_secs_f64());
        let rate = if elapsed > 0.0 {
            (self.done as f64 / elapsed) as u64
        } else {
            0
        };
        let eta_seconds = (rate > 0).then(|| (self.total - self.done).div_ceil(rate));
        Snapshot {
            current_item: self.current.clone(),
            bytes_done: self.done,
            bytes_total: self.total,
            rate,
            eta_seconds,
        }
    }
}

/// The `transfer` notification for `id` in `state`.
pub fn notification(id: &TransferId, state: State, snapshot: &Snapshot) -> Value {
    json!({
        "transfer_id": hex(id),
        "state": state.name(),
        "current_item": snapshot.current_item,
        "bytes_done": snapshot.bytes_done,
        "bytes_total": snapshot.bytes_total,
        "rate": snapshot.rate,
        "eta_seconds": snapshot.eta_seconds,
    })
}
