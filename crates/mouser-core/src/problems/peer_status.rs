//! Peer status exchange (Story 8 scenario 1): each computer tells its
//! desk-mates its permissions, limitations, Secure Input holder and problems
//! when it joins the desk and whenever any of them changes, so the health
//! panel on any computer shows every computer's state.

use std::collections::BTreeMap;

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, PeerStatus, SecureInputHolder};

use crate::control::status::{permission_kind, permission_state};
use crate::engine::{Ctx, Phase};

impl Ctx {
    /// What this computer tells its desk-mates about itself. Problems about
    /// other computers are theirs to tell.
    pub(crate) fn peer_status(&self) -> PeerStatus {
        PeerStatus {
            permissions: self
                .platform
                .permissions()
                .into_iter()
                .map(|(k, s)| (permission_kind(k), permission_state(s)))
                .collect::<BTreeMap<_, _>>(),
            limitations: self.limitations().into_iter().map(|l| l.text).collect(),
            secure_input_holder: self.platform.secure_input().map(|h| SecureInputHolder {
                pid: h.pid,
                app_name: h.app_name,
            }),
            problems: self
                .problems
                .open()
                .into_iter()
                .filter(|p| p.computer == self.me())
                .collect(),
        }
    }

    /// Send the desk-mates our status if it is not what they last heard.
    pub(crate) fn sync_peer_status(&self) {
        let status = self.peer_status();
        let mut core = self.core.lock().unwrap();
        if core.status_sent.as_ref() == Some(&status) {
            return;
        }
        core.status_sent = Some(status.clone());
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::Status(status.clone()));
        }
    }

    /// A desk-mate's status: shown in `status`; its Secure Input holder and
    /// new problems are news for our user too.
    pub(crate) fn on_peer_status(
        &self,
        peer: ComputerId,
        status: PeerStatus,
    ) -> Result<(), String> {
        let mut core = self.core.lock().unwrap();
        if !Self::authed(&core, peer) {
            return Err("Status from a computer that is not on the desk".into());
        }
        let entry = core.peers.get_mut(&peer).expect("authed");
        let name = entry.hello.display_name.clone();
        let old = entry.status.replace(status.clone());
        drop(core);
        if old.as_ref() == Some(&status) {
            return Ok(());
        }
        let old_holder = old.as_ref().map(|s| &s.secure_input_holder);
        if old_holder != Some(&status.secure_input_holder) {
            self.show_secure_input(peer, &name, status.secure_input_holder.as_ref());
        }
        self.announce_new_problems(old.as_ref(), &status);
        self.notify_status();
        Ok(())
    }
}
