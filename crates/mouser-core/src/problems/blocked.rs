//! The block explanation (Story 5 scenario 4): the first blocked crossing
//! tells the desk user what blocked it and how to cross anyway or change the
//! setting. The notice stays open in the registry, so later blocks do not
//! repeat it, until the desk user has seen it: they follow its fix, or
//! cross that edge (by the double bump, or once nothing blocks it).
//!
//! There is one notice per edge. Its `problem_id` is
//! `crossing_blocked:<computer id>/<display key>/<side>` (side is `left`,
//! `right`, `top` or `bottom`), which is how `open_arranger_edge` says which
//! edge to open.

use mouser_proto::control::{FixAction, ProblemKind};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DisplayId, Side};

use super::Problems;

/// What stopped the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Corner,
    Fullscreen,
}

fn subject(display: &DisplayId, side: Side) -> String {
    let side = serde_json::to_value(side).expect("side serializes");
    format!(
        "{}/{}/{}",
        display.computer,
        display.key,
        side.as_str().expect("side is a string")
    )
}

/// The edge crossed: its notice has done its job.
pub fn crossed(problems: &Problems, display: &DisplayId, side: Side) {
    problems.clear(ProblemKind::CrossingBlocked, &subject(display, side));
}

/// Raise the explanation for this edge unless it was already shown.
pub fn explain(
    problems: &Problems,
    me: ComputerId,
    reason: Reason,
    display: &DisplayId,
    side: Side,
) {
    let what = match reason {
        Reason::Corner => "The pointer is in a screen corner, and corners do not cross.",
        Reason::Fullscreen => "A full-screen application is in front, and it blocks crossing.",
    };
    problems.raise(
        ProblemKind::CrossingBlocked,
        &subject(display, side),
        me,
        format!(
            "{what} To cross anyway, push the same edge twice in a row. \
             To change this, open the edge in the display arranger."
        ),
        Some(FixAction::OpenArrangerEdge),
    );
}
