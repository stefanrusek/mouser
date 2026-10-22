//! Interruption (data-model "Transfer"): the other computer went offline
//! mid-copy. Both sides move the transfer to `Failed(peer_offline)` and tell
//! the user how many files made it. Nothing is retried; the destination's
//! stream task deletes the file that was being written, completed files
//! stay.

use std::time::Instant;

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::TransferId;
use serde_json::Value;

use super::engine::{Out, State, Transfers, file_count, total_size};
use super::progress::{self, Progress, Snapshot};

/// The reason in the `transfer` notification.
pub const PEER_OFFLINE: &str = "peer_offline";

/// The `transfer` notification of a copy that stopped after `copied` of
/// `total` files.
fn failed(id: &TransferId, snapshot: &Snapshot, name: &str, copied: usize, total: usize) -> Out {
    let mut note: Value = progress::notification(id, State::Failed, snapshot);
    note["reason"] = PEER_OFFLINE.into();
    let (file, were) = if total == 1 {
        ("file", "was")
    } else {
        ("files", "were")
    };
    note["message"] = format!(
        "The copy stopped because {name} went offline. {copied} of {total} {file} {were} copied."
    )
    .into();
    Out::Notify(note)
}

impl Transfers {
    /// The source: the destination has everything of one more file.
    pub(super) fn on_sent(&mut self, id: TransferId) {
        // A pasted-files transfer is not ours to count.
        if let Some(transfer) = self.outbound.get_mut(&id) {
            transfer.sent += 1;
        }
    }

    /// `peer` is gone: its copies that were going on fail, the rest is
    /// forgotten.
    pub(super) fn peer_gone(&mut self, peer: ComputerId, name: &str, now: Instant) -> Vec<Out> {
        let mut outs = Vec::new();
        let mut sending: Vec<_> = (self.outbound.iter())
            .filter(|(_, t)| t.to == peer)
            .map(|(id, _)| *id)
            .collect();
        sending.sort_by_key(|id| id.0);
        for id in sending {
            let transfer = self.outbound.remove(&id).expect("listed above");
            self.ids.remove(&id);
            let items: Vec<_> = transfer.files.iter().map(|(i, _)| i.clone()).collect();
            let snapshot = Progress::new(total_size(&items)).snapshot(now);
            outs.push(failed(
                &id,
                &snapshot,
                name,
                transfer.sent,
                file_count(&items),
            ));
        }
        let mut receiving: Vec<_> = (self.inbound.iter())
            .filter(|(_, t)| t.from == peer)
            .map(|(id, _)| *id)
            .collect();
        receiving.sort_by_key(|id| id.0);
        for id in receiving {
            let transfer = self.inbound.remove(&id).expect("listed above");
            if !transfer.state.in_progress() {
                continue;
            }
            self.ids.remove(&id);
            self.failed.insert(id);
            let total = file_count(&transfer.items);
            let copied = total - transfer.remaining - transfer.skipped;
            outs.push(Out::Forget { id });
            outs.push(failed(
                &id,
                &transfer.progress.snapshot(now),
                name,
                copied,
                total,
            ));
        }
        outs
    }
}
