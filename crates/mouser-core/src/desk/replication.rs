//! `DeskState` replication: the highest `(epoch, computer_id)` wins.

use std::sync::Arc;

use mouser_proto::ids::ComputerId;
use mouser_proto::model::DeskState;

use crate::engine::Ctx;

/// Replace `current` with `incoming` if it supersedes it; true if replaced.
pub fn adopt(current: &mut DeskState, incoming: DeskState) -> bool {
    let wins = incoming.supersedes(current);
    if wins {
        *current = incoming;
    }
    wins
}

impl Ctx {
    /// A desk-mate sent its desk state.
    pub(crate) fn on_desk_state(
        self: &Arc<Self>,
        peer: ComputerId,
        incoming: DeskState,
    ) -> Result<(), String> {
        let mut core = self.core.lock().unwrap();
        if !Self::authed(&core, peer) {
            return Err("DeskState from a computer that is not on the desk".into());
        }
        let desk = core.state.desk.as_mut().expect("authed peers imply a desk");
        if adopt(&mut desk.state, incoming) {
            self.persist(&core);
            drop(core);
            self.notify_status();
        }
        Ok(())
    }
}
