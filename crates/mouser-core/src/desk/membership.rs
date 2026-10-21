//! Leave, remove and move, with desk-key rotation (Story 1 scenario 5).
//!
//! When a computer leaves or is removed, the lowest-numbered remaining member
//! issues the new state (epoch + 1) and a fresh random desk key, and hands
//! both to the other remaining members; the departed computer never gets them.

use std::sync::Arc;

use mouser_proto::control::RpcError;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DeskState, Member};
use mouser_proto::wire::ControlMessage;

use crate::control::invalid;
use crate::desk::create::random;
use crate::desk::join::{Sealed, seal};
use crate::engine::{Core, Ctx, Phase};
use crate::state::Membership;

/// `state` plus `member`, one epoch later, authored by `author`.
pub fn with_member(state: &DeskState, member: Member, author: ComputerId) -> DeskState {
    let mut next = state.clone();
    next.members.push(member);
    next.epoch += 1;
    next.author = author;
    next
}

/// `state` without `who`, one epoch later, authored by `author`.
pub fn without_member(state: &DeskState, who: ComputerId, author: ComputerId) -> DeskState {
    let mut next = state.clone();
    next.members.retain(|m| m.computer_id != who);
    next.epoch += 1;
    next.author = author;
    next
}

/// The member that issues the rotation: the lowest `ComputerId`.
pub fn rotator(state: &DeskState) -> ComputerId {
    state
        .members
        .iter()
        .map(|m| m.computer_id)
        .min()
        .expect("a desk has members")
}

impl Ctx {
    /// Tell every desk-mate we are leaving and drop their connections.
    pub(crate) fn say_goodbye(&self, core: &mut Core) {
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::Leave {
                computer_id: self.me(),
            });
            peer.close();
        }
    }

    /// Become unjoined, forgetting the desk and its key.
    fn forget_desk(&self, core: &mut Core) {
        let desk = core.state.desk.take().expect("on a desk");
        self.delete_desk_key(&desk.desk_id);
        core.desk_key = None;
        core.state.membership = Membership::Unjoined;
        core.pending = None;
        core.requests = Default::default();
        self.persist(core);
    }

    /// Install `state` with a fresh desk key and hand both to the other
    /// authenticated members (not `skip`).
    fn rotate(&self, core: &mut Core, state: DeskState, skip: ComputerId) {
        let key: [u8; 32] = random();
        let desk = core.state.desk.as_mut().expect("on a desk");
        desk.state = state.clone();
        let (id, name) = (desk.desk_id, desk.name.clone());
        core.desk_key = Some(key);
        self.store_desk_key(&id, &key);
        self.persist(core);
        let sealed = seal(&Sealed { key, name, state });
        for (_, peer) in core
            .peers
            .iter()
            .filter(|(id, p)| **id != skip && p.phase == Phase::Authed)
        {
            peer.send(ControlMessage::JoinDecision {
                approved: true,
                sealed: Some(sealed.clone()),
            });
        }
    }

    /// `desk.leave`.
    pub(crate) fn leave_desk(self: &Arc<Self>) -> Result<(), RpcError> {
        {
            let mut core = self.core.lock().unwrap();
            if core.state.desk.is_none() {
                return Err(invalid("this computer is not on a desk"));
            }
            self.say_goodbye(&mut core);
            self.forget_desk(&mut core);
        }
        self.announce();
        self.notify_status();
        Ok(())
    }

    /// `desk.remove_member`.
    pub(crate) fn remove_member(self: &Arc<Self>, who: ComputerId) -> Result<(), RpcError> {
        {
            let mut core = self.core.lock().unwrap();
            let desk = core
                .state
                .desk
                .as_ref()
                .ok_or_else(|| invalid("this computer is not on a desk"))?;
            if who == self.me() || !desk.state.members.iter().any(|m| m.computer_id == who) {
                return Err(invalid("that computer is not another member of this desk"));
            }
            let state = without_member(&desk.state, who, self.me());
            self.rotate(&mut core, state, who);
            if let Some(removed) = core.peers.get(&who) {
                removed.send(ControlMessage::Remove { computer_id: who });
                removed.close();
            }
        }
        self.notify_status();
        Ok(())
    }

    /// A desk-mate announces it is leaving.
    pub(crate) fn on_leave(
        self: &Arc<Self>,
        peer: ComputerId,
        who: ComputerId,
    ) -> Result<(), String> {
        let mut core = self.core.lock().unwrap();
        if who != peer || !Self::authed(&core, peer) {
            return Err(format!("invalid Leave from {peer}"));
        }
        let desk = core.state.desk.as_ref().expect("authed peers imply a desk");
        let state = without_member(&desk.state, who, self.me());
        if rotator(&state) == self.me() {
            self.rotate(&mut core, state, who);
        }
        core.peers[&peer].close();
        drop(core);
        self.notify_status();
        Ok(())
    }

    /// A desk-mate removed us.
    pub(crate) fn on_remove(
        self: &Arc<Self>,
        peer: ComputerId,
        who: ComputerId,
    ) -> Result<(), String> {
        let mut core = self.core.lock().unwrap();
        if who != self.me() || !Self::authed(&core, peer) {
            return Err(format!("invalid Remove from {peer}"));
        }
        for other in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            other.close();
        }
        self.forget_desk(&mut core);
        drop(core);
        self.announce();
        self.notify_status();
        Ok(())
    }
}
