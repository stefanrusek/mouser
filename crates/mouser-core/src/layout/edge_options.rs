//! Per-edge options (Story 5 scenario 5): an edge without an entry has the
//! defaults, and an entry applies to its (display, side) alone.

use mouser_proto::model::{DisplayId, EdgeOptions, Side};

/// Whether corners of `side` of `display` are blocked (default `true`).
pub fn block_corners(options: &[EdgeOptions], display: &DisplayId, side: Side) -> bool {
    options
        .iter()
        .find(|o| &o.display_id == display && o.side == side)
        .is_none_or(|o| o.block_corners)
}
