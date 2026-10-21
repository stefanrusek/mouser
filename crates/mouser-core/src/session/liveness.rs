//! Peer liveness (data-model "ConnectionState", research R2).
//!
//! A desk-mate whose link ended without the desk being left is *reconnecting*:
//! the transport declares a path lost after the 3 s idle timeout, and the
//! computer is dialled again until it answers. Absent for longer than
//! [`OFFLINE_GRACE`] it is *offline*: the health panel says so, with when it
//! was last seen, and a `peer_offline` problem is raised. Any authenticated
//! connection (a rediscovery, a re-dial, the peer dialling us) returns it to
//! *connected* and clears the problem.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use mouser_proto::control::{ConnectionState, ProblemKind};
use mouser_proto::ids::ComputerId;

use crate::engine::{Core, Ctx, Phase};

/// How long a lost computer stays *reconnecting* before it is *offline*.
pub const OFFLINE_GRACE: Duration = Duration::from_secs(5);

/// A desk-mate we had a connection to and no longer have.
pub(crate) struct Absent {
    pub since: Instant,
    /// When the computer was last heard from (RFC 3339).
    pub last_seen: String,
}

pub(crate) type AbsentPeers = BTreeMap<ComputerId, Absent>;

impl Core {
    /// `peer` is a member of the desk this computer is on.
    pub fn is_member(&self, peer: ComputerId) -> bool {
        self.state
            .desk
            .as_ref()
            .is_some_and(|d| d.state.members.iter().any(|m| m.computer_id == peer))
    }

    /// Note that the connection to `peer` was lost, keeping the earliest
    /// time if it already was.
    pub fn mark_absent(&mut self, peer: ComputerId, last_seen: String, now: Instant) {
        self.absent.entry(peer).or_insert(Absent {
            since: now,
            last_seen,
        });
    }

    /// Where the connection to `peer` stands at `now`.
    pub fn connection_of(&self, peer: ComputerId, now: Instant) -> ConnectionState {
        match self.peers.get(&peer).map(|p| p.phase) {
            Some(Phase::Authed) => ConnectionState::Connected,
            Some(Phase::Incompatible) => ConnectionState::Incompatible,
            _ => match self.absent.get(&peer) {
                Some(absent) if now.duration_since(absent.since) < OFFLINE_GRACE => {
                    ConnectionState::Reconnecting
                }
                _ => ConnectionState::Offline,
            },
        }
    }
}

impl Ctx {
    /// `peer` proved desk membership again: it is connected and no longer
    /// offline.
    pub(crate) fn mark_present(&self, core: &mut Core, peer: ComputerId) {
        core.absent.remove(&peer);
        self.problems
            .clear(ProblemKind::PeerOffline, &peer.to_string());
    }

    /// Raise `peer_offline` for computers absent past the grace period, and
    /// forget absences of computers that are no longer on the desk. Run on
    /// every reconcile tick.
    pub(crate) fn check_liveness(&self) {
        let now = Instant::now();
        let mut changed = false;
        let mut core = self.core.lock().unwrap();
        let gone: Vec<ComputerId> = core
            .absent
            .keys()
            .copied()
            .filter(|peer| !core.is_member(*peer))
            .collect();
        for peer in gone {
            core.absent.remove(&peer);
            changed |= self
                .problems
                .clear(ProblemKind::PeerOffline, &peer.to_string());
        }
        for (peer, absent) in &core.absent {
            if now.duration_since(absent.since) >= OFFLINE_GRACE {
                let name = core
                    .state
                    .desk
                    .as_ref()
                    .and_then(|d| d.state.members.iter().find(|m| m.computer_id == *peer))
                    .map(|m| m.display_name.clone())
                    .expect("absent computers are members");
                changed |= self.problems.raise(
                    ProblemKind::PeerOffline,
                    &peer.to_string(),
                    *peer,
                    format!(
                        "{name} is offline. Make sure it is turned on, awake and on the same network."
                    ),
                    None,
                );
            }
        }
        drop(core);
        if changed {
            self.notify_status();
        }
    }
}
