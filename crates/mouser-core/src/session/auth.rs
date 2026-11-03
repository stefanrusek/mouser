//! `DeskAuth`: every new connection proves desk membership with
//! HMAC(desk_key, handshake transcript). The transcript is the TLS exporter
//! of the connection (unique per connection), the desk id and the sender, so a
//! proof cannot be replayed on another connection or reflected to its sender.

use crate::lock::LockExt;
use hmac::{Hmac, KeyInit, Mac};
use mouser_proto::ids::{ComputerId, DeskId};
use quinn::Connection;
use sha2::Sha256;

fn mac(key: &[u8; 32], exporter: &[u8; 32], desk: &DeskId, sender: &ComputerId) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(exporter);
    mac.update(&desk.0);
    mac.update(&sender.0);
    mac
}

/// The connection-unique part of the transcript.
pub fn export(conn: &Connection) -> [u8; 32] {
    let mut out = [0u8; 32];
    conn.export_keying_material(&mut out, b"mouser/deskauth", b"")
        .expect("TLS exporter available");
    out
}

/// The `DeskAuth` HMAC `sender` sends.
pub fn proof(key: &[u8; 32], exporter: &[u8; 32], desk: &DeskId, sender: &ComputerId) -> [u8; 32] {
    mac(key, exporter, desk, sender)
        .finalize()
        .into_bytes()
        .into()
}

/// Check a `DeskAuth` HMAC received from `sender`.
pub fn verify(
    key: &[u8; 32],
    exporter: &[u8; 32],
    desk: &DeskId,
    sender: &ComputerId,
    received: &[u8; 32],
) -> bool {
    mac(key, exporter, desk, sender)
        .verify_slice(received)
        .is_ok()
}

use std::sync::Arc;

use mouser_proto::control::ProblemKind;
use mouser_proto::model::DeskState;
use mouser_proto::wire::ControlMessage;

use crate::desk::membership::{rotator, with_member, without_member};

use crate::engine::{Core, Ctx, Phase};
use crate::logging::events;
use crate::session::close_reason::CloseReason;

impl Ctx {
    /// Send our `DeskAuth` proof to `peer` (we are on a desk and so is it).
    pub(crate) fn send_auth(&self, core: &Core, peer: ComputerId) {
        let desk = core.state.desk.as_ref().expect("on a desk").desk_id;
        let key = core.desk_key.expect("desk key stored");
        let entry = &core.peers[&peer];
        let hmac = proof(&key, &export(&entry.conn), &desk, &self.me());
        entry.send(ControlMessage::DeskAuth { hmac });
    }

    /// Whether `peer` has proven desk membership.
    pub(crate) fn authed(core: &Core, peer: ComputerId) -> bool {
        core.peers
            .get(&peer)
            .is_some_and(|p| p.phase == Phase::Authed)
    }

    /// Verify the peer's proof. A wrong proof closes the connection: a
    /// computer without the desk key gets no desk traffic.
    pub(crate) fn on_desk_auth(
        self: &Arc<Self>,
        peer: ComputerId,
        hmac: [u8; 32],
    ) -> Result<(), String> {
        let mut core = self.core.guard();
        let (Some(key), Some(desk)) = (core.desk_key, core.state.desk.as_ref().map(|d| d.desk_id))
        else {
            return Err("DeskAuth while not on a desk".into());
        };
        let listed = core.is_member(peer);
        let entry = core.peers.get_mut(&peer).expect("peer registered");
        if !verify(&key, &export(&entry.conn), &desk, &peer, &hmac) {
            entry.close_for(CloseReason::NotMember);
            let name = entry.hello.display_name.clone();
            drop(core);
            // A computer this desk lists that cannot prove it holds the desk
            // key (it was offline when the key was rotated) will never get
            // in by trying again: say so once, until it does.
            if listed {
                self.problems.raise(
                    ProblemKind::JoinRefused,
                    &peer.to_string(),
                    peer,
                    format!("{name} is on this desk but does not hold its key; it must join the desk again."),
                    None,
                );
            }
            return Ok(());
        }
        entry.phase = Phase::Authed;
        // The session's old connection, if this one took it over, is done.
        if let Some(previous) = entry.previous.take() {
            previous.close(CloseReason::Replaced);
        }
        self.log.write(events::peer_connected(
            peer,
            &entry.hello.mouser_version,
            entry.hello.os,
        ));
        core.refusals.accepted(peer);
        self.problems
            .clear(ProblemKind::JoinRefused, &peer.to_string(), peer);
        self.mark_present(&mut core, peer);
        let state = self.refreshed_entry(&mut core, peer);
        let displays = self.local_displays(&core);
        core.peers[&peer].send(ControlMessage::DeskState(state));
        core.peers[&peer].send(ControlMessage::Displays { displays });
        core.peers[&peer].send_status(&self.peer_status());
        self.send_settings(&core, peer);
        self.tell_paused(&core, peer);
        self.offer_release(&core, peer);
        self.refresh_map(&core);
        drop(core);
        self.notify_status();
        Ok(())
    }
}

impl Ctx {
    /// The desk state to send `peer`, once it has proven membership. If it
    /// runs another version or goes by another name than the desk records,
    /// the desk-mate that would issue the state were it to leave (the one
    /// `on_leave` makes it) records what it says, under a new epoch, and
    /// tells every other desk-mate. The layout is untouched, so nothing
    /// is re-placed by it.
    fn refreshed_entry(&self, core: &mut Core, peer: ComputerId) -> DeskState {
        let desk = core.state.desk.as_mut().expect("on a desk");
        let hello = &core.peers[&peer].hello;
        // A holder of the desk key that the state does not list (a state
        // that lost it, see `on_desk_state`) is recorded when it is back.
        let Some(recorded) = desk.state.members.iter().find(|m| m.computer_id == peer) else {
            return desk.state.clone();
        };
        // The name is the desk's to give (a rename made while the computer
        // was away is on the desk before the computer says its old one), so
        // only the version is taken from the computer.
        let changed = recorded.mouser_version != hello.mouser_version;
        let refresher =
            changed && rotator(&without_member(&desk.state, peer, self.me())) == self.me();
        if !refresher {
            return desk.state.clone();
        }
        let mut member = recorded.clone();
        member.mouser_version = hello.mouser_version.clone();
        desk.state = with_member(&desk.state, member, self.me());
        let state = desk.state.clone();
        let _ = self.persist(core);
        for (id, other) in core.peers.iter() {
            if *id != peer && other.phase == Phase::Authed {
                other.send(ControlMessage::DeskState(state.clone()));
            }
        }
        state
    }
}
