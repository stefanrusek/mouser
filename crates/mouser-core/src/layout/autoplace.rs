//! Auto-placement: a computer's displays, in their local arrangement, go to
//! the right of everything already placed, top-aligned (Story 2 scenario 1).

use std::collections::{BTreeMap, BTreeSet};

use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, Layout, Placement};

use super::edges::{Placed, desk_size};

/// Placements for `new` (one computer's displays) to the right of `kept`'s
/// bounding box and aligned with its top; at the origin when nothing is kept.
pub fn place(kept: &[Placed], new: &[Display]) -> Vec<Placement> {
    let right = kept.iter().map(|p| p.rect.right()).max().unwrap_or(0);
    let top = kept.iter().map(|p| p.rect.y).min().unwrap_or(0);
    let Some(anchor) = new.iter().min_by_key(|d| d.origin_local) else {
        return Vec::new();
    };
    // The local arrangement is in logical pixels; the anchor's density turns
    // distances between displays into desk units.
    let (aw, ah) = desk_size(anchor);
    let (sx, sy) = (
        f64::from(aw) / f64::from(anchor.logical_size.0),
        f64::from(ah) / f64::from(anchor.logical_size.1),
    );
    let min_x = new.iter().map(|d| d.origin_local.0).min().unwrap_or(0);
    let min_y = new.iter().map(|d| d.origin_local.1).min().unwrap_or(0);
    new.iter()
        .map(|d| Placement {
            display_id: d.display_id.clone(),
            x: right + (f64::from(d.origin_local.0 - min_x) * sx).round() as i32,
            y: top + (f64::from(d.origin_local.1 - min_y) * sy).round() as i32,
        })
        .collect()
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

use super::edges;
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
        core.state.displays.insert(me, self.local_displays());
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
        let stale: BTreeSet<ComputerId> = known
            .keys()
            .filter(|c| placed_keys.get(*c).cloned().unwrap_or_default() != wanted(c))
            .copied()
            .collect();
        let unknown = placed_keys.keys().any(|c| !known.contains_key(c));
        if stale.is_empty() || unknown {
            return;
        }
        let mut layout = old.clone();
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
            let placements = place(&kept, &displays);
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
