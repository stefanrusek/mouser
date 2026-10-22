//! Drag hand-off (Story 7 scenario 1, decision D3 item 3): when the pointer
//! crosses to another computer while files are being dragged, the files are
//! offered to it; there the platform starts a drag of them and reports where
//! they were dropped.
//!
//! A platform without file drag (Linux) gets none of this; its limitation is
//! reported with the others (see `problems::limitations`).

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;

use super::engine::{Command, Ids};
use crate::crossing::Crossing;

/// The `destination_dir` of a dragged-files offer: the destination decides,
/// by where the drop lands. (Clipboard files offer it empty.)
pub const DRAG_DESTINATION: &str = "drop";

/// Whether `message` belongs to a dragged-files transfer rather than to
/// files being pasted: offers say so, the rest follow their transfer's id.
pub fn is_drag(message: &ControlMessage, ids: &Ids) -> bool {
    match message {
        ControlMessage::TransferOffer {
            destination_dir, ..
        } => destination_dir == DRAG_DESTINATION,
        ControlMessage::TransferAccept { transfer_id }
        | ControlMessage::TransferDone { transfer_id } => ids.contains(transfer_id),
        _ => true,
    }
}

impl Crossing {
    /// The pointer has just crossed to `to`: offer it the files being dragged.
    pub(crate) fn hand_off_drag(&mut self, to: ComputerId) {
        let Some(paths) = self.drag.as_mut().and_then(|drag| drag.outgoing()) else {
            return;
        };
        // The offer follows the `Enter` on the same ordered stream.
        let _ = self.transfers.send(Command::Drag { to, paths });
    }
}
