//! Equalized dimensions (Story 4 scenario 4): crossing maps the full length
//! of one edge onto the full length of the adjoining edge, so displays of
//! different physical sizes line up proportionally.

use mouser_proto::model::Side;

use super::edges::{self, Placed, Step, clamp_into, exit};

/// The span of `side` of `p` on the axis along it.
fn span(p: &Placed, side: Side) -> (f64, f64) {
    match side {
        Side::Left | Side::Right => (f64::from(p.rect.y), f64::from(p.rect.bottom())),
        Side::Top | Side::Bottom => (f64::from(p.rect.x), f64::from(p.rect.right())),
    }
}

/// [`edges::step`], mapping a crossing proportionally when `equalize`.
pub fn step(placed: &[Placed], from: usize, at: (f64, f64), equalize: bool) -> Step {
    let plain = edges::step(placed, from, at);
    if !equalize || plain == Step::Stay {
        return plain;
    }
    let (side, along) = exit(placed[from].rect, at);
    let distance = |e: &edges::Edge| {
        (f64::from(e.lo) - along)
            .max(along - f64::from(e.hi))
            .max(0.0)
    };
    let Some(edge) = edges::edges(placed)
        .into_iter()
        .filter(|e| e.from == from && e.side == side)
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
    else {
        return plain;
    };
    let (from_lo, from_hi) = span(&placed[from], side);
    let (to_lo, to_hi) = span(&placed[edge.to], side);
    let fraction = ((along - from_lo) / (from_hi - from_lo)).clamp(0.0, 1.0);
    let mapped = to_lo + fraction * (to_hi - to_lo);
    let wanted = match side {
        Side::Left | Side::Right => (at.0, mapped),
        Side::Top | Side::Bottom => (mapped, at.1),
    };
    Step::Cross {
        to: edge.to,
        at: clamp_into(placed[edge.to].rect, wanted),
    }
}
