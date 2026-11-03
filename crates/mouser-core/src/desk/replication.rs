//! `DeskState` replication: the highest `(epoch, computer_id)` wins.

use crate::lock::LockExt;
use std::sync::Arc;

use mouser_proto::control::SettingOrigin;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DeskState, Member};

use crate::desk::membership::keep_newest_entries;
use crate::engine::{Core, Ctx, Phase};
use mouser_proto::wire::ControlMessage;

/// Replace `current` with `incoming` if it supersedes it; true if replaced.
pub fn adopt(current: &mut DeskState, incoming: DeskState) -> bool {
    let wins = incoming.supersedes(current);
    if wins {
        *current = incoming;
        keep_newest_entries(&mut current.members);
    }
    wins
}

/// A desk state that crossed with ours may lack members we have: nothing a
/// desk-mate sends as a `DeskState` removes a member (a removal or a leave
/// goes with a new desk key, in a sealed state), so those members were
/// never removed, only unknown to the state's author. They are put back,
/// and the state is ours under a new epoch, which the desk-mates take in
/// turn. True if any were put back.
pub fn restore_members(state: &mut DeskState, had: Vec<Member>, me: ComputerId) -> bool {
    let missing: Vec<Member> = had
        .into_iter()
        .filter(|m| !state.members.iter().any(|n| n.computer_id == m.computer_id))
        .collect();
    let lost = !missing.is_empty();
    if lost {
        state.members.extend(missing);
        state.epoch += 1;
        state.author = me;
    }
    lost
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
        let had = desk.state.members.clone();
        if adopt(&mut desk.state, incoming) {
            let lost = restore_members(&mut desk.state, had, self.me());
            let state = desk.state.clone();
            self.layout_changed(&mut core, &old, &state);
            self.adopt_shared_config(&mut core);
            let renamed = self.adopt_own_name(&mut core, &state);
            self.sync_layout_of(&mut core, Some(self.me()));
            if lost {
                let state = core.state.desk.as_ref().expect("on a desk").state.clone();
                for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
                    peer.send(ControlMessage::DeskState(state.clone()));
                }
            }
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
