//! Corner blocking (Story 5 scenario 1): motion past an edge within the
//! corner of either end of it does not cross, so aiming at a hot corner or a
//! window control does not throw the pointer onto another computer.

use mouser_proto::model::Side;

use crate::layout::edges::{self, Placed};

/// The fixed corner size, in logical pixels along the edge.
pub const CORNER_PX: f64 = 8.0;

/// Whether the point `along` (desk coordinate on the axis along `side`) is
/// within the corner of either end of `side` of `display`.
pub fn in_corner(display: &Placed, side: Side, along: f64) -> bool {
    let (lo, hi, per_px) = match side {
        Side::Left | Side::Right => (
            display.rect.y,
            display.rect.bottom(),
            edges::mm_per_px(display).1,
        ),
        Side::Top | Side::Bottom => (
            display.rect.x,
            display.rect.right(),
            edges::mm_per_px(display).0,
        ),
    };
    let corner = CORNER_PX * per_px;
    along < f64::from(lo) + corner || along >= f64::from(hi) - corner
}
