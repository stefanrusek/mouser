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

use mouser_proto::wire::ControlMessage;

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
        let entry = core.peers.get_mut(&peer).expect("peer registered");
        if !verify(&key, &export(&entry.conn), &desk, &peer, &hmac) {
            entry.close_for(CloseReason::NotMember);
            return Ok(());
        }
        entry.phase = Phase::Authed;
        self.log.write(events::peer_connected(
            peer,
            &entry.hello.mouser_version,
            entry.hello.os,
        ));
        self.mark_present(&mut core, peer);
        let state = core.state.desk.as_ref().expect("on a desk").state.clone();
        let displays = self.local_displays(&core);
        core.peers[&peer].send(ControlMessage::DeskState(state));
        core.peers[&peer].send(ControlMessage::Displays { displays });
        core.peers[&peer].send(ControlMessage::Status(self.peer_status()));
        self.tell_paused(&core, peer);
        self.offer_release(&core, peer);
        self.refresh_map(&core);
        drop(core);
        self.notify_status();
        Ok(())
    }
}
