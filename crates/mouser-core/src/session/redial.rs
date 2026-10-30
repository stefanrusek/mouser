//! Re-dial after this computer's own address changed (Story 3 scenario 3,
//! research R2).
//!
//! QUIC migrates a connection only from the side that dialled it
//! (`Endpoint::rebind`, done in `transport::netchange`). A connection this
//! computer *accepted* cannot follow its own address change: the peer keeps
//! sending to the old address. So the accepting side dials the peer again,
//! repeats `Hello` and `DeskAuth`, and the new connection replaces the old
//! one, resuming the session.

use crate::lock::LockExt;
use std::sync::Arc;

use mouser_proto::ids::ComputerId;

use crate::engine::{Ctx, Phase};

impl Ctx {
    /// Our address changed: every desk-mate whose connection we accepted
    /// is to be dialled again.
    pub(crate) fn redial_accepted(self: &Arc<Self>) {
        let mut core = self.core.guard();
        let accepted: Vec<_> = core
            .peers
            .iter()
            .filter(|(_, p)| !p.dialed && p.phase == Phase::Authed)
            .map(|(id, _)| *id)
            .collect();
        core.redial.extend(accepted);
        drop(core);
        self.redial_pending();
    }

    /// A connection from `peer`, accepted across our address change, has
    /// been taken over: it is to be dialled again, once it is proven.
    pub(crate) fn redial_moved(self: &Arc<Self>, peer: ComputerId) {
        self.core.guard().redial.insert(peer);
        self.redial_pending();
    }

    /// Dial the computers still to be dialled again, until the new
    /// connection is up. A computer whose connection has ended is left to
    /// the ordinary reconnection (the lower `ComputerId` dials), so a pair
    /// never dials each other at once; one that is not visible on the
    /// network is tried again on the next tick. An accepted connection
    /// still proving membership is dialled again once it has.
    pub(crate) fn redial_pending(self: &Arc<Self>) {
        let found = self.discovery.found();
        let mut core = self.core.guard();
        let pending: Vec<_> = core.redial.iter().copied().collect();
        for peer in pending {
            let accepted = core.peers.get(&peer).filter(|p| !p.dialed).map(|p| p.phase);
            match accepted {
                Some(Phase::Authed) => {
                    if let Some(f) = found.iter().find(|f| f.computer == peer)
                        && core.dialing.insert(peer)
                    {
                        self.spawn(self.clone().dial_found(f.clone()));
                    }
                }
                Some(Phase::Greeted) => {}
                _ => {
                    core.redial.remove(&peer);
                }
            }
        }
    }
}
