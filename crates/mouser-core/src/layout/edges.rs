//! Desk-space geometry: display rectangles, the edges derived from them, and
//! mapping a point that leaves a display onto the display it enters.

use mouser_proto::model::{Display, DisplayId, Layout, Side};

/// Pointer positions are kept just inside a rectangle's far edges.
const INSET: f64 = 1e-6;
/// Millimetres per logical pixel at 96 dpi.
const MM_PER_96DPI_PX: f64 = 25.4 / 96.0;

/// An integer rectangle in desk space (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && x < f64::from(self.right())
            && y >= f64::from(self.y)
            && y < f64::from(self.bottom())
    }
}

/// A display with its rectangle in desk space.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub display: Display,
    pub rect: Rect,
}

/// The size of a display in desk units: its physical size in millimetres when
/// known, else its logical pixels at 96 dpi.
pub fn desk_size(display: &Display) -> (i32, i32) {
    let (w, h) = display.physical_size_mm.unwrap_or((
        (f64::from(display.logical_size.0) * MM_PER_96DPI_PX) as f32,
        (f64::from(display.logical_size.1) * MM_PER_96DPI_PX) as f32,
    ));
    (w.round() as i32, h.round() as i32)
}

/// The included displays that have a placement, in desk space.
pub fn placed(displays: &[Display], layout: &Layout) -> Vec<Placed> {
    displays
        .iter()
        .filter(|d| d.included)
        .filter_map(|d| {
            let at = layout
                .placements
                .iter()
                .find(|p| p.display_id == d.display_id)?;
            let (w, h) = desk_size(d);
            Some(Placed {
                display: d.clone(),
                rect: Rect {
                    x: at.x,
                    y: at.y,
                    w,
                    h,
                },
            })
        })
        .collect()
}

/// Where two displays adjoin: `from`'s `side` meets `to` along `lo..hi` of
/// the shared boundary (desk coordinates on the axis along the edge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub side: Side,
    pub to: usize,
    pub lo: i32,
    pub hi: i32,
}

/// Every adjoining pair, in both directions: rectangles that share a boundary
/// segment of length greater than zero.
pub fn edges(placed: &[Placed]) -> Vec<Edge> {
    let mut found = Vec::new();
    for (i, a) in placed.iter().enumerate() {
        for (j, b) in placed.iter().enumerate().filter(|(j, _)| *j != i) {
            let (a, b) = (a.rect, b.rect);
            let vertical = (a.y.max(b.y), a.bottom().min(b.bottom()));
            let horizontal = (a.x.max(b.x), a.right().min(b.right()));
            if a.right() == b.x && vertical.0 < vertical.1 {
                found.push(Edge {
                    from: i,
                    side: Side::Right,
                    to: j,
                    lo: vertical.0,
                    hi: vertical.1,
                });
                found.push(Edge {
                    from: j,
                    side: Side::Left,
                    to: i,
                    lo: vertical.0,
                    hi: vertical.1,
                });
            }
            if a.bottom() == b.y && horizontal.0 < horizontal.1 {
                found.push(Edge {
                    from: i,
                    side: Side::Bottom,
                    to: j,
                    lo: horizontal.0,
                    hi: horizontal.1,
                });
                found.push(Edge {
                    from: j,
                    side: Side::Top,
                    to: i,
                    lo: horizontal.0,
                    hi: horizontal.1,
                });
            }
        }
    }
    found
}

/// A point of `display` (logical pixels from its top-left) in desk space.
pub fn to_desk(p: &Placed, local: (f64, f64)) -> (f64, f64) {
    let (lw, lh) = (
        f64::from(p.display.logical_size.0),
        f64::from(p.display.logical_size.1),
    );
    (
        f64::from(p.rect.x) + local.0 * f64::from(p.rect.w) / lw,
        f64::from(p.rect.y) + local.1 * f64::from(p.rect.h) / lh,
    )
}

/// A desk-space point as logical pixels from the top-left of `display`,
/// kept on the display.
pub fn to_local(p: &Placed, desk: (f64, f64)) -> (f64, f64) {
    let (lw, lh) = (
        f64::from(p.display.logical_size.0),
        f64::from(p.display.logical_size.1),
    );
    (
        ((desk.0 - f64::from(p.rect.x)) * lw / f64::from(p.rect.w)).clamp(0.0, lw - 1.0),
        ((desk.1 - f64::from(p.rect.y)) * lh / f64::from(p.rect.h)).clamp(0.0, lh - 1.0),
    )
}

/// Desk millimetres per logical pixel of `display`, horizontally and vertically.
pub fn mm_per_px(p: &Placed) -> (f64, f64) {
    (
        f64::from(p.rect.w) / f64::from(p.display.logical_size.0),
        f64::from(p.rect.h) / f64::from(p.display.logical_size.1),
    )
}

/// What a pointer at desk point `at`, having been on display `from`, does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Still on the display.
    Stay,
    /// Crossed onto display `to`, now at `at` (inside it).
    Cross { to: usize, at: (f64, f64) },
    /// Left the display where nothing adjoins: held at its edge.
    Clamp { at: (f64, f64) },
}

fn clamp_into(rect: Rect, at: (f64, f64)) -> (f64, f64) {
    (
        at.0.clamp(f64::from(rect.x), f64::from(rect.right()) - INSET),
        at.1.clamp(f64::from(rect.y), f64::from(rect.bottom()) - INSET),
    )
}

/// Move the pointer of display `from` to desk point `at`.
pub fn step(placed: &[Placed], from: usize, at: (f64, f64)) -> Step {
    let rect = placed[from].rect;
    if rect.contains(at.0, at.1) {
        return Step::Stay;
    }
    let (side, along) = if at.0 >= f64::from(rect.right()) {
        (Side::Right, at.1)
    } else if at.0 < f64::from(rect.x) {
        (Side::Left, at.1)
    } else if at.1 >= f64::from(rect.bottom()) {
        (Side::Bottom, at.0)
    } else {
        (Side::Top, at.0)
    };
    let edge = edges(placed).into_iter().find(|e| {
        e.from == from && e.side == side && f64::from(e.lo) <= along && along < f64::from(e.hi)
    });
    match edge {
        Some(e) => Step::Cross {
            to: e.to,
            at: clamp_into(placed[e.to].rect, at),
        },
        None => Step::Clamp {
            at: clamp_into(rect, at),
        },
    }
}

/// Index of the display `id` in `placed`.
pub fn index_of(placed: &[Placed], id: &DisplayId) -> Option<usize> {
    placed.iter().position(|p| &p.display.display_id == id)
}
