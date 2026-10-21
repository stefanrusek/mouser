//! Display exchange: a member's local displays as wire `Display`s.

use std::sync::Arc;

use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId};
use mouser_proto::wire::ControlMessage;

use crate::engine::{Ctx, Phase};

/// The wire form of `displays`, all included in the layout.
pub fn to_wire(computer: ComputerId, displays: &[LocalDisplay]) -> Vec<Display> {
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
            included: true,
        })
        .collect()
}

impl Ctx {
    /// This computer's displays in wire form.
    pub(crate) fn local_displays(&self) -> Vec<Display> {
        to_wire(
            self.me(),
            &self.platform.displays().expect("displays readable"),
        )
    }

    /// A peer reports its displays: they replace what we knew, and the
    /// layout follows.
    pub(crate) fn on_displays(
        self: &Arc<Self>,
        peer: ComputerId,
        displays: Vec<Display>,
    ) -> Result<(), String> {
        let mut core = self.core.lock().unwrap();
        if !Self::authed(&core, peer) {
            return Err("Displays from a computer that is not on the desk".into());
        }
        core.peers.get_mut(&peer).expect("authed").displays = displays.clone();
        core.state.displays.insert(peer, displays);
        self.sync_layout(&mut core);
        self.persist(&core);
        self.refresh_map(&core);
        drop(core);
        self.notify_status();
        Ok(())
    }

    /// This computer's displays changed (a monitor was plugged in, the
    /// laptop was docked): tell the desk, and place them.
    pub(crate) fn on_local_displays(self: &Arc<Self>) {
        let mut core = self.core.lock().unwrap();
        self.sync_layout(&mut core);
        self.persist(&core);
        self.refresh_map(&core);
        let displays = self.local_displays();
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::Displays {
                displays: displays.clone(),
            });
        }
        drop(core);
        self.notify_status();
    }
}
