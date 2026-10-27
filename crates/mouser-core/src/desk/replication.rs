//! `DeskState` replication: the highest `(epoch, computer_id)` wins.

use crate::lock::LockExt;
use std::sync::Arc;

use mouser_proto::control::SettingOrigin;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DeskState;

use crate::engine::{Core, Ctx};

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
        let mut core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("DeskState from a computer that is not on the desk".into());
        }
        let desk = core.state.desk.as_mut().expect("authed peers imply a desk");
        let old = desk.state.layout.clone();
        if adopt(&mut desk.state, incoming) {
            let state = desk.state.clone();
            self.layout_changed(&mut core, &old, &state);
            self.adopt_shared_config(&mut core);
            let renamed = self.adopt_own_name(&mut core, &state);
            self.sync_layout(&mut core);
            drop(core);
            if renamed {
                self.announce();
            }
            self.notify_status();
        }
        Ok(())
    }

    /// The desk gave this computer another name than its settings hold
    /// (it was renamed from another computer, perhaps while this one was
    /// offline): the desk's name becomes this computer's own. True if it
    /// did.
    fn adopt_own_name(&self, core: &mut Core, state: &DeskState) -> bool {
        let me = self.me();
        let named = state
            .members
            .iter()
            .find(|m| m.computer_id == me)
            .map(|m| m.display_name.clone())
            .filter(|name| *name != self.display_name_in(core));
        let Some(name) = named else {
            return false;
        };
        self.store_own_name(core, &name, SettingOrigin::DeskMate, Some(state.author));
        true
    }
}
