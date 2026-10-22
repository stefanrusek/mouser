//! Auto-placement: a computer's displays, in their local arrangement, go to
//! the right of the rightmost display already placed, top-aligned with it
//! (Story 2 scenario 1), so that the two computers share an edge.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, Layout, Placement};

use super::edges::{self, Placed, Rect, desk_size};
use super::validate;

/// Placements for `new` (one computer's displays): its leftmost display
/// adjoins the right side of `kept`'s rightmost display, tops aligned; with
/// nothing kept, the displays start at the origin.
pub fn place(kept: &[Placed], new: &[Display]) -> Vec<Placement> {
    let arranged = arrange(new);
    let Some(contact) = arranged.iter().min_by_key(|(_, x, y)| (*x, *y)) else {
        return Vec::new();
    };
    let (dx, dy) = match kept.iter().max_by_key(|p| (p.rect.right(), -p.rect.y)) {
        Some(target) => (target.rect.right() - contact.1, target.rect.y - contact.2),
        None => (
            -arranged.iter().map(|a| a.1).min().unwrap_or(0),
            -arranged.iter().map(|a| a.2).min().unwrap_or(0),
        ),
    };
    arranged
        .into_iter()
        .map(|(d, x, y)| Placement {
            display_id: d.display_id.clone(),
            x: x + dx,
            y: y + dy,
        })
        .collect()
}

/// Like [`place`], for a computer that already has `old` placements: the
/// displays take the arrangement the OS reports now, anchored on the
/// persisting display that is first in it, so the computer keeps its place
/// among the others. With no persisting display, or when that would overlap
/// a kept display or leave the computer touching none, they are [`place`]d
/// afresh.
pub fn reposition(kept: &[Placed], old: &[Placement], new: &[Display]) -> Vec<Placement> {
    let arranged = arrange(new);
    let anchor = arranged
        .iter()
        .filter(|(d, ..)| old.iter().any(|p| p.display_id == d.display_id))
        .min_by_key(|(d, ..)| d.origin_local);
    let Some((anchor, ax, ay)) = anchor else {
        return place(kept, new);
    };
    let at = old
        .iter()
        .find(|p| p.display_id == anchor.display_id)
        .expect("the anchor has a placement");
    let (dx, dy) = (at.x - ax, at.y - ay);
    let moved: Vec<Placed> = arranged
        .into_iter()
        .map(|(d, x, y)| {
            let (w, h) = desk_size(d);
            Placed {
                display: d.clone(),
                rect: Rect {
                    x: x + dx,
                    y: y + dy,
                    w,
                    h,
                },
            }
        })
        .collect();
    let overlapping = moved
        .iter()
        .any(|m| kept.iter().any(|k| edges::overlap(&k.rect, &m.rect)));
    let together: Vec<Placed> = moved.iter().chain(kept).cloned().collect();
    let touching = kept.is_empty()
        || edges::edges(&together)
            .iter()
            .any(|e| (e.from < moved.len()) != (e.to < moved.len()));
    if overlapping || !touching {
        return place(kept, new);
    }
    moved
        .into_iter()
        .map(|m| Placement {
            display_id: m.display.display_id,
            x: m.rect.x,
            y: m.rect.y,
        })
        .collect()
}

/// A display's rectangle in its computer's own coordinates (points).
fn local_rect(d: &Display) -> (i32, i32, i32, i32) {
    let (x, y) = d.origin_local;
    (
        x,
        y,
        x + d.logical_size.0.round() as i32,
        y + d.logical_size.1.round() as i32,
    )
}

/// Desk millimetres per point of `d`, across and down.
fn density(d: &Display) -> (f64, f64) {
    let (w, h) = desk_size(d);
    (
        f64::from(w) / f64::from(d.logical_size.0),
        f64::from(h) / f64::from(d.logical_size.1),
    )
}

/// `new` laid out in desk units, relative to an arbitrary origin. Displays
/// that share an edge locally share it in desk space too: each is placed
/// against a neighbour already placed, offset along the edge at that
/// neighbour's density, since one density for all of them would open gaps
/// between displays of different densities. A display touching none keeps
/// its offset from the first, at the first's density.
pub fn arrange(new: &[Display]) -> Vec<(&Display, i32, i32)> {
    let Some(first) = (0..new.len()).min_by_key(|&i| new[i].origin_local) else {
        return Vec::new();
    };
    let mut at: Vec<Option<(i32, i32)>> = vec![None; new.len()];
    at[first] = Some((0, 0));
    let mut queue = VecDeque::from([first]);
    while let Some(i) = queue.pop_front() {
        let (a, (ax, ay)) = (&new[i], at[i].expect("queued displays are placed"));
        let (al, at_, ar, ab) = local_rect(a);
        let (aw, ah) = desk_size(a);
        let (sx, sy) = density(a);
        let along = |delta: i32, s: f64| (f64::from(delta) * s).round() as i32;
        for j in 0..new.len() {
            if at[j].is_some() {
                continue;
            }
            let b = &new[j];
            let (bl, bt, br, bb) = local_rect(b);
            let (bw, bh) = desk_size(b);
            let rows = at_.max(bt) < ab.min(bb);
            let columns = al.max(bl) < ar.min(br);
            let pos = if rows && bl == ar {
                Some((ax + aw, ay + along(bt - at_, sy)))
            } else if rows && br == al {
                Some((ax - bw, ay + along(bt - at_, sy)))
            } else if columns && bt == ab {
                Some((ax + along(bl - al, sx), ay + ah))
            } else if columns && bb == at_ {
                Some((ax + along(bl - al, sx), ay - bh))
            } else {
                None
            };
            if pos.is_some() {
                at[j] = pos;
                queue.push_back(j);
            }
        }
    }
    let (fx, fy) = new[first].origin_local;
    let (sx, sy) = density(&new[first]);
    new.iter()
        .zip(at)
        .map(|(d, pos)| {
            let (x, y) = pos.unwrap_or((
                (f64::from(d.origin_local.0 - fx) * sx).round() as i32,
                (f64::from(d.origin_local.1 - fy) * sy).round() as i32,
            ));
            (d, x, y)
        })
        .collect()
}

/// The computers none of whose displays adjoin another computer's display:
/// the pointer cannot reach or leave them. Empty when fewer than two
/// computers are placed.
pub fn isolated(placed: &[Placed]) -> BTreeSet<ComputerId> {
    let computer = |i: usize| placed[i].display.display_id.computer;
    let all: BTreeSet<ComputerId> = (0..placed.len()).map(computer).collect();
    if all.len() < 2 {
        return BTreeSet::new();
    }
    let reached: BTreeSet<ComputerId> = edges::edges(placed)
        .into_iter()
        .filter(|e| computer(e.from) != computer(e.to))
        .map(|e| computer(e.from))
        .collect();
    all.difference(&reached).copied().collect()
}

fn keys_by_computer(layout: &Layout) -> BTreeMap<ComputerId, BTreeSet<&str>> {
    let mut keys: BTreeMap<ComputerId, BTreeSet<&str>> = BTreeMap::new();
    for p in &layout.placements {
        keys.entry(p.display_id.computer)
            .or_default()
            .insert(&p.display_id.key);
    }
    keys
}

/// The computers whose displays are placed differently (new, replaced or
/// gone) in `new` than in `old`, when `old` already had other displays placed:
/// the desk owner is offered a review of them.
pub fn review_candidates(old: &Layout, new: &Layout) -> Vec<ComputerId> {
    if old.placements.is_empty() {
        return Vec::new();
    }
    let (before, after) = (keys_by_computer(old), keys_by_computer(new));
    after
        .iter()
        .filter(|(computer, keys)| before.get(computer) != Some(keys))
        .map(|(computer, _)| *computer)
        .collect()
}

use mouser_proto::wire::ControlMessage;

use crate::engine::{Core, Ctx, EngineEvent, Phase};

impl Ctx {
    /// Bring the layout in line with the displays every member reported:
    /// a member whose displays are not the ones placed (new, or replaced
    /// after docking or undocking) gets them placed by [`place`]. Members
    /// compute this alike from the same reports, so the results agree and
    /// the highest `(epoch, author)` wins. Deferred while the size of a
    /// placed display is not known.
    pub(crate) fn sync_layout(&self, core: &mut Core) {
        let me = self.me();
        let mine = self.local_displays(core);
        core.state.displays.insert(me, mine);
        let Some(desk) = core.state.desk.as_mut() else {
            return;
        };
        let members: BTreeSet<ComputerId> =
            desk.state.members.iter().map(|m| m.computer_id).collect();
        core.state.displays.retain(|c, _| members.contains(c));
        let known = &core.state.displays;
        let old = desk.state.layout.clone();
        let placed_keys = keys_by_computer(&old);
        let wanted = |c: &ComputerId| -> BTreeSet<&str> {
            known[c]
                .iter()
                .filter(|d| d.included)
                .map(|d| d.display_id.key.as_str())
                .collect()
        };
        // Placements of displays their owner removed from the layout are
        // the desk owner's doing (the placement follows with the layout, and
        // the two can arrive in either order): they are no reason to place
        // a computer again.
        let placed_for = |c: &ComputerId| -> BTreeSet<&str> {
            placed_keys
                .get(c)
                .into_iter()
                .flatten()
                .copied()
                .filter(|k| {
                    !known[c]
                        .iter()
                        .any(|d| !d.included && d.display_id.key == *k)
                })
                .collect()
        };
        let unknown = placed_keys.keys().any(|c| !known.contains_key(c));
        if unknown {
            return;
        }
        // A computer with no edge to the others (an earlier version could
        // place one so) is placed again, as if new.
        let all_displays: Vec<_> = known.values().flatten().cloned().collect();
        let cut_off = isolated(&edges::placed(&all_displays, &old));
        let stale: BTreeSet<ComputerId> = known
            .keys()
            .filter(|c| {
                placed_for(c) != wanted(c)
                    || cut_off.contains(*c)
                    || !validate::rigid(&known[*c], &old)
            })
            .copied()
            .collect();
        if stale.is_empty() {
            return;
        }
        let mut layout = old.clone();
        let before: Vec<Placement> = layout
            .placements
            .iter()
            .filter(|p| stale.contains(&p.display_id.computer))
            .cloned()
            .collect();
        layout
            .placements
            .retain(|p| !stale.contains(&p.display_id.computer));
        layout
            .edge_options
            .retain(|e| !stale.contains(&e.display_id.computer));
        let kept_displays: Vec<_> = known
            .iter()
            .filter(|(c, _)| !stale.contains(*c))
            .flat_map(|(_, ds)| ds.iter().cloned())
            .collect();
        let mut kept = edges::placed(&kept_displays, &layout);
        for computer in &stale {
            let displays: Vec<_> = known[computer]
                .iter()
                .filter(|d| d.included)
                .cloned()
                .collect();
            // A computer cut off from the others is placed as if new; any
            // other keeps its place among them.
            let placements = if cut_off.contains(computer) {
                place(&kept, &displays)
            } else {
                reposition(&kept, &before, &displays)
            };
            kept.extend(edges::placed(
                &displays,
                &Layout {
                    placements: placements.clone(),
                    ..Layout::default()
                },
            ));
            layout.placements.extend(placements);
        }
        let state = &mut desk.state;
        state.layout = layout;
        state.epoch += 1;
        state.author = me;
        let state = state.clone();
        self.layout_changed(core, &old, &state);
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::DeskState(state.clone()));
        }
    }

    /// The desk state changed from `old` layout: persist, offer reviews,
    /// and rebuild the input path's map.
    pub(crate) fn layout_changed(
        &self,
        core: &mut Core,
        old: &Layout,
        state: &mouser_proto::model::DeskState,
    ) {
        for computer in review_candidates(old, &state.layout) {
            let name = state
                .members
                .iter()
                .find(|m| m.computer_id == computer)
                .map(|m| m.display_name.clone())
                .expect("a placed computer is a member");
            core.needs_review.extend(
                state
                    .layout
                    .placements
                    .iter()
                    .filter(|p| p.display_id.computer == computer)
                    .map(|p| p.display_id.clone()),
            );
            // No subscribers is fine.
            let _ = self
                .events
                .send(EngineEvent::ReviewOffered { computer, name });
        }
        core.needs_review
            .retain(|d| state.layout.placements.iter().any(|p| &p.display_id == d));
        self.persist(core);
        self.refresh_map(core);
    }
}
