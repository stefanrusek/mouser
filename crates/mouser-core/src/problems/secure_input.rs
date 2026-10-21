//! macOS Secure Input (Story 2 scenario 10): an application holding it blocks
//! Mouser from typing on that computer. The desk user is told which one, on
//! that computer and on every desk-mate; the connection stays up.

use std::collections::BTreeMap;

use mouser_proto::control::ProblemKind;
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, PeerStatus, SecureInputHolder};

use crate::control::status::{permission_kind, permission_state};
use crate::engine::{Ctx, Phase};

fn message(holder: &SecureInputHolder, computer: &str) -> String {
    format!(
        "{app} is blocking Mouser from typing on {computer} (Secure Input). \
         Switch away from {app} or close it and typing works again.",
        app = holder.app_name
    )
}

impl Ctx {
    /// What this computer tells its desk-mates about itself.
    pub(crate) fn peer_status(&self) -> PeerStatus {
        PeerStatus {
            permissions: self
                .platform
                .permissions()
                .into_iter()
                .map(|(k, s)| (permission_kind(k), permission_state(s)))
                .collect::<BTreeMap<_, _>>(),
            limitations: self
                .platform
                .info()
                .limitations
                .into_iter()
                .map(|l| l.text)
                .collect(),
            secure_input_holder: self.platform.secure_input().map(|h| SecureInputHolder {
                pid: h.pid,
                app_name: h.app_name,
            }),
            problems: self.problems.open(),
        }
    }

    /// Raise (or clear) the Secure Input problem of `computer`.
    fn show_secure_input(
        &self,
        computer: ComputerId,
        name: &str,
        holder: Option<&SecureInputHolder>,
    ) {
        let subject = computer.to_string();
        self.problems.clear(ProblemKind::SecureInput, &subject);
        if let Some(holder) = holder {
            self.problems.raise(
                ProblemKind::SecureInput,
                &subject,
                computer,
                message(holder, name),
                None,
            );
        }
    }

    /// Poll this computer's Secure Input holder; on a change, tell the user
    /// and the desk-mates.
    pub(crate) fn sync_secure_input(&self) {
        let holder = self.platform.secure_input();
        let name = {
            let mut core = self.core.lock().unwrap();
            if core.secure_holder == holder {
                return;
            }
            core.secure_holder = holder;
            self.display_name_in(&core)
        };
        let status = self.peer_status();
        self.show_secure_input(self.me(), &name, status.secure_input_holder.as_ref());
        let core = self.core.lock().unwrap();
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::Status(status.clone()));
        }
    }

    /// A desk-mate's status: its Secure Input holder is news for our user too.
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
        let changed = entry.status.as_ref().map(|s| &s.secure_input_holder)
            != Some(&status.secure_input_holder);
        let name = entry.hello.display_name.clone();
        let holder = status.secure_input_holder.clone();
        entry.status = Some(status);
        drop(core);
        if changed {
            self.show_secure_input(peer, &name, holder.as_ref());
        }
        Ok(())
    }
}
