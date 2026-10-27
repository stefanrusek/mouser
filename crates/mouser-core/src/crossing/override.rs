//! Deciding a blocked crossing (Story 5): corners and full-screen
//! applications block it, and a deliberate double bump of the same edge
//! overrides the block.

use std::time::{Duration, Instant};

use mouser_proto::model::{DisplayId, Side};

use super::Crossing;
use super::corners;
use crate::layout::edge_options;
use crate::layout::edges;
use crate::problems::blocked::{self, Reason};

/// A second hit on the same edge within this long of the first crosses.
pub const WINDOW: Duration = Duration::from_millis(600);

type Edge = (DisplayId, Side);

/// The bumps against blocked edges.
#[derive(Debug, Default)]
pub struct Bumps {
    /// The last bump that did not override, and when.
    last: Option<(Edge, Instant)>,
    /// The edge the pointer has been pushing against since it last moved
    /// elsewhere: one push is one bump however many events it takes.
    touching: Option<Edge>,
}

impl Bumps {
    /// The pointer pushes against a blocked `edge`. True when this is the
    /// second bump and the block gives way.
    pub fn hit(&mut self, edge: Edge, now: Instant) -> bool {
        if self.touching.as_ref() == Some(&edge) {
            return false;
        }
        match self.last.take() {
            Some((last, at)) if last == edge && now.duration_since(at) <= WINDOW => {
                self.touching = None;
                true
            }
            _ => {
                self.last = Some((edge.clone(), now));
                self.touching = Some(edge);
                false
            }
        }
    }

    /// The pointer is not pushing against a blocked edge.
    pub fn release(&mut self) {
        self.touching = None;
    }
}

impl Crossing {
    /// Whether the pointer may leave display `from` for the point `wanted`
    /// (desk space): not blocked, or blocked and overridden by a double bump.
    /// `local` is whether the pointer is on this computer's own display, the
    /// only one whose foreground application is known.
    pub(super) fn may_cross(
        &mut self,
        from: usize,
        wanted: (f64, f64),
        local: bool,
        now: Instant,
    ) -> bool {
        let source = &self.map.placed[from];
        let (side, along) = edges::exit(source.rect, wanted);
        let id = source.display.display_id.clone();
        let reason = if edge_options::block_corners(&self.map.edge_options, &id, side)
            && corners::in_corner(source, side, along)
        {
            Reason::Corner
        } else if local && self.fullscreen_blocks() {
            Reason::Fullscreen
        } else {
            self.bumps.release();
            blocked::crossed(&self.problems, self.me, &id, side);
            return true;
        };
        if self.bumps.hit((id.clone(), side), now) {
            blocked::crossed(&self.problems, self.me, &id, side);
            return true;
        }
        blocked::explain(&self.problems, self.me, reason, &id, side);
        false
    }
}
