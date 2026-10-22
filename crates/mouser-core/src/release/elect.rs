//! Which computer goes to the internet for releases (FR-003): the online
//! member with the lowest `ComputerId`. Nobody else does.

use mouser_proto::ids::ComputerId;

use crate::engine::{Core, Phase};

/// The fetcher among `me` and the other `online` members.
pub fn elect(me: ComputerId, online: impl IntoIterator<Item = ComputerId>) -> ComputerId {
    online.into_iter().chain([me]).min().expect("includes me")
}

impl Core {
    /// Whether `me` is the elected fetcher among the members it is
    /// connected to.
    pub(crate) fn is_fetcher(&self, me: ComputerId) -> bool {
        let online = self
            .peers
            .iter()
            .filter(|(_, p)| p.phase == Phase::Authed)
            .map(|(id, _)| *id);
        elect(me, online) == me
    }
}
