//! The block explanation (Story 5 scenario 4): the first blocked crossing
//! tells the desk user what blocked it and how to cross anyway or change the
//! setting. The notice stays open in the registry, so later blocks do not
//! repeat it.

use mouser_proto::control::{FixAction, ProblemKind};
use mouser_proto::ids::ComputerId;

use super::Problems;

/// What stopped the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Corner,
    Fullscreen,
}

const SUBJECT: &str = "crossing";

/// Raise the explanation unless it was already shown.
pub fn explain(problems: &Problems, me: ComputerId, reason: Reason) {
    let what = match reason {
        Reason::Corner => "The pointer is in a screen corner, and corners do not cross.",
        Reason::Fullscreen => "A full-screen application is in front, and it blocks crossing.",
    };
    problems.raise(
        ProblemKind::CrossingBlocked,
        SUBJECT,
        me,
        format!(
            "{what} To cross anyway, push the same edge twice in a row. \
             To change this, open the edge in the display arranger."
        ),
        Some(FixAction::OpenArrangerEdge),
    );
}
