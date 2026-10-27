//! Pause and resume (Story 2 scenario 12): a paused computer is isolated in
//! both directions as if disconnected, but its connections stay up so that
//! resuming is instant.
//!
//! * Its own pointer and keyboard never leave it (no edge crossing, no
//!   QuickJump, no clipboard hotkeys), and control that is elsewhere comes
//!   back.
//! * Nobody can cross onto it or send it input: it refuses `Enter`, ends what
//!   it was receiving (releasing every key the controller held), and tells
//!   desk-mates running 0.2.2 or later with `Paused`, whose displays then
//!   count as unavailable on their side, like those of an offline computer.
//! * Clipboard and file transfers stop, as when a desk-mate is lost; on
//!   resume the current clipboard is offered again, both ways.

use crate::lock::LockExt;
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;

use super::Crossing;
use super::controller::Return;
use crate::clipboard::Command;
use crate::engine::{Core, Ctx, Phase};
use crate::logging::events::ControlHow;
use crate::release::Version;

/// The first Mouser that decodes `Paused`. An older peer treats an unknown
/// message as a malformed frame, so it is never sent one.
const PAUSE_SINCE: &str = "0.2.2";

impl Crossing {
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.return_local(Return::Release, ControlHow::Pause, None);
            self.end_receiving();
            if self.jump.take().is_some() {
                self.hide_letters();
            }
            if self.letters_from.is_some() {
                self.on_quickjump_hide();
            }
        }
    }
}

impl Ctx {
    /// `active.pause` / `active.resume`.
    pub(crate) fn set_paused(&self, paused: bool) {
        let mut core = self.core.guard();
        core.paused = paused;
        self.crossing.guard().set_paused(paused);
        for (id, peer) in core.peers.iter().filter(|(_, p)| p.phase == Phase::Authed) {
            if Self::speaks_paused(&peer.hello.mouser_version) {
                peer.send(ControlMessage::Paused { paused });
            }
            if paused {
                self.forget(*id, &core.name_of(*id));
            }
        }
        // No clipboard task means this platform shares no clipboard.
        let _ = self.clip.send(Command::Paused(paused));
        drop(core);
        self.notify_status();
    }

    /// Tell `peer` we are paused, if we are and it understands.
    pub(crate) fn tell_paused(&self, core: &Core, peer: ComputerId) {
        let entry = &core.peers[&peer];
        if core.paused && Self::speaks_paused(&entry.hello.mouser_version) {
            entry.send(ControlMessage::Paused { paused: true });
        }
    }

    /// `Paused` from `peer`: its displays leave (or rejoin) the layout the
    /// input path sees, and nothing is shared with it while it is paused.
    pub(crate) fn on_peer_paused(&self, peer: ComputerId, paused: bool) -> Result<(), String> {
        let mut core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("Paused from a computer that is not on the desk".into());
        }
        let name = core.name_of(peer);
        let entry = core.peers.get_mut(&peer).expect("authed");
        entry.paused = paused;
        self.refresh_map(&core);
        drop(core);
        if paused {
            self.forget(peer, &name);
        } else {
            let _ = self.clip.send(Command::PeerResumed(peer));
        }
        self.notify_status();
        Ok(())
    }

    /// Whatever `peer` still owed us or we owed it is dropped, as if it were
    /// disconnected.
    fn forget(&self, peer: ComputerId, name: &str) {
        let _ = self.clip.send(Command::PeerGone(peer));
        let _ = self.xfer.send(crate::transfer::engine::Command::PeerGone(
            peer,
            name.to_string(),
        ));
    }

    /// True when the peer running `version` understands `Paused`.
    fn speaks_paused(version: &str) -> bool {
        let since = Version::parse(PAUSE_SINCE).expect("a version");
        Version::parse(version).is_ok_and(|v| v >= since)
    }

    /// True when sharing with `peer` is off: either side is paused.
    pub(crate) fn sharing_off(&self, core: &Core, peer: ComputerId) -> bool {
        core.paused || core.peers.get(&peer).is_some_and(|p| p.paused)
    }
}
