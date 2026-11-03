//! Computers that refused this one as "not a member of this desk" (T544).
//!
//! A refusal is not mended by dialling again at once: each further refusal
//! from the same computer doubles the wait before the next dial, up to the
//! longest reconnect wait, and a successful handshake forgets them.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use mouser_proto::ids::ComputerId;

use super::DIAL_ATTEMPT;
use super::liveness::OFFLINE_GRACE;

/// The wait after the `count`th refusal in a row.
pub fn wait_after(count: u32) -> Duration {
    DIAL_ATTEMPT
        .saturating_mul(1u32 << (count - 1).min(16))
        .min(OFFLINE_GRACE)
}

/// How often each computer refused us in a row, and until when it is left be.
#[derive(Default)]
pub(crate) struct Refusals(BTreeMap<ComputerId, (u32, Instant)>);

impl Refusals {
    /// `peer` refused us at `now`.
    pub fn refused(&mut self, peer: ComputerId, now: Instant) {
        let count = self.0.get(&peer).map_or(1, |(count, _)| count + 1);
        self.0.insert(peer, (count, now + wait_after(count)));
    }

    /// `peer` is not to be dialled yet at `now`.
    pub fn waiting(&self, peer: ComputerId, now: Instant) -> bool {
        self.0.get(&peer).is_some_and(|(_, until)| now < *until)
    }

    /// A handshake with `peer` succeeded.
    pub fn accepted(&mut self, peer: ComputerId) {
        self.0.remove(&peer);
    }
}
