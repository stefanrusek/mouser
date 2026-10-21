//! `layout.get` / `layout.set` (Story 4, contracts/control-api.md).
//!
//! The result is the desk's `Layout` plus `displays`: every display with its
//! details (see [`crate::layout::details`]). `layout.set` takes a `Layout`
//! and, optionally, `displays`, a list of `{display_id, included}` that
//! removes or restores displays of this computer (Story 4 scenario 3);
//! entries that change nothing are ignored, so what `layout.get` returned
//! can be sent back with edits. The layout is validated before anything
//! changes; then it takes a new epoch and goes to every desk-mate.

use std::sync::Arc;

use mouser_proto::control::RpcError;
use mouser_proto::model::{Display, Layout};
use mouser_proto::wire::ControlMessage;
use serde::Deserialize;
use serde_json::Value;

use crate::config;
use crate::control::invalid;
use crate::engine::{Core, Ctx, Phase};
use crate::layout::remove::{self, Inclusion};
use crate::layout::validate::validate;

/// The params of `layout.set`.
#[derive(Debug, Deserialize)]
pub struct LayoutSetParams {
    #[serde(flatten)]
    pub layout: Layout,
    #[serde(default)]
    pub displays: Vec<Inclusion>,
}

impl Ctx {
    /// `layout.get`.
    pub(crate) fn get_layout(&self) -> Value {
        let core = self.core.lock().unwrap();
        self.layout_value(&core)
    }

    fn layout_value(&self, core: &Core) -> Value {
        let layout = core
            .state
            .desk
            .as_ref()
            .map(|d| d.state.layout.clone())
            .unwrap_or_default();
        let mut value = serde_json::to_value(layout).expect("layout serializes");
        value["displays"] =
            serde_json::to_value(self.display_details(core)).expect("details serialize");
        value
    }

    /// `layout.set`: refuse an invalid layout and keep the previous one.
    pub(crate) fn set_layout(self: &Arc<Self>, params: LayoutSetParams) -> Result<Value, RpcError> {
        let mut core = self.core.lock().unwrap();
        if core.state.desk.is_none() {
            return Err(invalid("This computer is not on a desk"));
        }
        let me = self.me();
        let mut removed = core.state.removed.clone();
        let mut known = core.state.displays.clone();
        known.insert(me, self.local_displays_without(&removed));
        remove::apply(me, &mut known, &mut removed, &params.displays).map_err(invalid)?;
        let everything: Vec<Display> = known.values().flatten().cloned().collect();
        validate(&everything, &params.layout).map_err(invalid)?;

        // The display list and the layout that goes with it arrive in either
        // order without harm when they agree; a removal is announced before
        // the layout without its placement, a restoration after the layout
        // that has it.
        let removal = !removed.is_subset(&core.state.removed);
        let displays_changed = removed != core.state.removed;
        core.state.removed = removed;
        core.state.displays = known;
        core.config.crossing.equalize_dimensions = params.layout.equalize_dimensions;
        core.config.crossing.block_fullscreen = params.layout.block_fullscreen;
        config::save(&self.config_dir, &core.config);
        let desk = core.state.desk.as_mut().expect("on a desk");
        let old = std::mem::replace(&mut desk.state.layout, params.layout);
        desk.state.epoch += 1;
        desk.state.author = me;
        let state = desk.state.clone();
        self.layout_changed(&mut core, &old, &state);
        let displays = self.local_displays(&core);
        let mut messages = vec![ControlMessage::DeskState(state)];
        if displays_changed {
            let announce = ControlMessage::Displays { displays };
            if removal {
                messages.insert(0, announce);
            } else {
                messages.push(announce);
            }
        }
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            for message in &messages {
                peer.send(message.clone());
            }
        }
        let value = self.layout_value(&core);
        drop(core);
        self.notify_status();
        Ok(value)
    }
}
