//! Name conflicts (Story 7 scenario 3): a file whose name is taken at the
//! destination waits for the user's answer; the rest of the transfer goes on
//! with the next file.

use std::time::Instant;

use mouser_proto::wire::{ControlMessage, TransferId};

use super::engine::{Decision, Out, State, Transfers};
use super::progress;
use super::streams::hex;

impl Transfers {
    /// The destination: the stream of item `index` arrived. Write it if its
    /// name is free, else ask.
    pub(super) fn on_item_start(
        &mut self,
        id: TransferId,
        index: usize,
        now: Instant,
    ) -> Result<Vec<Out>, String> {
        if self.failed.contains(&id) {
            return Ok(vec![Out::Forget { id }]);
        }
        let Some(transfer) = self.inbound.get_mut(&id) else {
            return Err("a file of a transfer nobody offered".into());
        };
        let name = transfer.items[index].relative_path.clone();
        transfer.progress.begin_item(&name);
        let path = transfer.dir.join(&name);
        if !path.exists() {
            return Ok(vec![Out::Decide {
                id,
                index,
                decision: Decision::Write(path),
            }]);
        }
        transfer.state = State::AwaitingOverwriteDecision;
        transfer.awaiting = Some(index);
        let from = transfer.from;
        let mut note = progress::notification(
            &id,
            State::AwaitingOverwriteDecision,
            &transfer.progress.snapshot(now),
        );
        note["item"] = name.clone().into();
        note["message"] = format!("{name} already exists there. Replace it?").into();
        Ok(vec![
            Out::To(
                from,
                ControlMessage::TransferConflict {
                    transfer_id: id,
                    item: name,
                },
            ),
            Out::Notify(note),
        ])
    }

    /// The user's answer, naming the transfer as its notification did.
    pub(super) fn resolve_named(
        &mut self,
        transfer_id: &str,
        overwrite: bool,
        now: Instant,
    ) -> Result<Vec<Out>, String> {
        let id = self
            .inbound
            .keys()
            .find(|id| hex(id) == transfer_id)
            .copied();
        let Some(id) = id else {
            return Err(format!("no transfer {transfer_id} to answer"));
        };
        self.resolve(id, overwrite, now)
    }

    /// Carry on with the waiting file, or leave it out.
    pub(super) fn resolve(
        &mut self,
        id: TransferId,
        overwrite: bool,
        now: Instant,
    ) -> Result<Vec<Out>, String> {
        let transfer = self.inbound.get_mut(&id).expect("checked by the caller");
        let Some(index) = transfer.awaiting.take() else {
            return Err("an answer to a question nobody asked".into());
        };
        let item = &transfer.items[index];
        let decision = if overwrite {
            Decision::Write(transfer.dir.join(&item.relative_path))
        } else {
            transfer.progress.skip(item.size);
            transfer.skipping = true;
            Decision::Skip
        };
        transfer.state = State::Copying;
        Ok(vec![
            Out::Decide {
                id,
                index,
                decision,
            },
            self.report(id, now),
        ])
    }
}
