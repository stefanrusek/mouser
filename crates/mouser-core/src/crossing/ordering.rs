//! Per-session input sequencing (Story 3 scenario 1, SC-002).
//!
//! Every message a controller sends carries the next `seq` of its session.
//! The control stream delivers in order, so on a live link each key, button
//! and scroll is newer than the one before; the sequencer is what lets the
//! receiver refuse anything that is not: a message that was queued on a link
//! that has since been replaced, or a motion datagram that overtook a newer
//! one on the network.

/// Admits only sequence numbers newer than every one admitted before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequencer {
    last: u64,
}

impl Sequencer {
    /// A session whose first message is numbered after `base` (the `seq_base`
    /// of its `Enter`).
    pub fn after(base: u64) -> Self {
        Self { last: base }
    }

    /// Whether `seq` is newer than everything admitted so far; if so it is
    /// now the newest.
    pub fn admit(&mut self, seq: u64) -> bool {
        if seq <= self.last {
            return false;
        }
        self.last = seq;
        true
    }
}
