//! Problem notifications (Story 8 scenario 3): a problem raised here is
//! announced by the registry itself with its message and at most one fix. A
//! problem a desk-mate reports about itself is announced the same way, once,
//! when it first appears in that computer's status.

use mouser_proto::control::ProblemKind;
use mouser_proto::wire::PeerStatus;

use crate::engine::Ctx;

impl Ctx {
    /// Announce each open problem in `new` that `old` did not have. Secure
    /// Input is left out: it is raised here as our own notice.
    pub(crate) fn announce_new_problems(&self, old: Option<&PeerStatus>, new: &PeerStatus) {
        for problem in new
            .problems
            .iter()
            .filter(|p| p.kind != ProblemKind::SecureInput)
            .filter(|p| {
                !old.is_some_and(|o| o.problems.iter().any(|q| q.problem_id == p.problem_id))
            })
        {
            self.problems.announce(problem);
        }
    }
}
