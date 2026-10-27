//! Display exchange: a member's local displays as wire `Display`s.

use crate::lock::LockExt;
use std::collections::BTreeSet;
use std::sync::Arc;

use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId};
use mouser_proto::wire::ControlMessage;

use crate::engine::{Core, Ctx, Phase};

/// The wire form of `displays`, included in the layout unless their key is
/// in `removed`.
pub fn to_wire(
    computer: ComputerId,
    displays: &[LocalDisplay],
    removed: &BTreeSet<String>,
) -> Vec<Display> {
    displays
        .iter()
        .map(|d| Display {
            display_id: DisplayId {
                computer,
                key: d.key.clone(),
            },
            name: d.name.clone(),
            pixel_size: d.pixel_size,
            logical_size: (d.logical_size.0 as f32, d.logical_size.1 as f32),
            scale: d.scale,
            physical_size_mm: d.physical_size_mm.map(|(w, h)| (w as f32, h as f32)),
            origin_local: (d.origin.0 as i32, d.origin.1 as i32),
            included: !removed.contains(&d.key),
        })
        .collect()
}

impl Ctx {
    /// This computer's displays in wire form.
    pub(crate) fn local_displays(&self, core: &Core) -> Vec<Display> {
        self.local_displays_without(&core.state.removed)
    }

    /// This computer's displays in wire form, those in `removed` taken out
    /// of the layout.
    pub(crate) fn local_displays_without(&self, removed: &BTreeSet<String>) -> Vec<Display> {
        to_wire(
            self.me(),
            &self.platform.displays().expect("displays readable"),
            removed,
        )
    }

    /// A peer reports its displays: they replace what we knew, and the
    /// layout follows.
    pub(crate) fn on_displays(
        self: &Arc<Self>,
        peer: ComputerId,
        displays: Vec<Display>,
    ) -> Result<(), String> {
        let mut core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("Displays from a computer that is not on the desk".into());
        }
        core.peers.get_mut(&peer).expect("authed").displays = displays.clone();
        core.state.displays.insert(peer, displays);
        self.sync_layout(&mut core);
        let _ = self.persist(&core);
        self.refresh_map(&core);
        drop(core);
        self.notify_status();
        Ok(())
    }

    /// This computer's displays changed (a monitor was plugged in, the
    /// laptop was docked): tell the desk, and place them.
    pub(crate) fn on_local_displays(self: &Arc<Self>) {
        let mut core = self.core.guard();
        self.sync_layout(&mut core);
        let _ = self.persist(&core);
        self.refresh_map(&core);
        let displays = self.local_displays(&core);
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::Displays {
                displays: displays.clone(),
            });
        }
        drop(core);
        self.notify_status();
    }
}
