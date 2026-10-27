//! Start at login and resume on start (Story 1 scenario 9, FR-011).

use crate::engine::Ctx;
use crate::lock::LockExt;
use crate::state::Membership;

impl Ctx {
    /// First-run is complete once this computer is on a desk: register (or
    /// not) for start at login as `general.start_at_login` says.
    pub(crate) fn complete_first_run(&self) {
        let on = self.core.guard().config.general.start_at_login;
        self.set_autostart(on);
    }

    /// On start, rejoin the persisted desk with no action: announce it so
    /// desk-mates dial us, and dial them. A join that never finished is over.
    pub(crate) fn resume(&self) {
        let pending = matches!(self.core.guard().state.membership, Membership::Pending(_));
        if pending {
            self.fail_pending("The request to join the desk was interrupted when Mouser stopped.");
        }
        {
            let mut core = self.core.guard();
            self.sync_layout(&mut core);
            let _ = self.persist(&core);
            self.refresh_map(&core);
        }
        self.announce();
    }
}

#[cfg(test)]
mod tests;
