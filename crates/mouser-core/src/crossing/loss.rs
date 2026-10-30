//! Control returns to this computer when the computer that had it is lost
//! (Story 3 scenario 5, edge case "no stuck keys").
//!
//! A computer is lost when its link ends: a path silent for the 3 s idle
//! timeout, a peer that shut down, a new connection that never proved
//! membership. The layout snapshot then no longer has it
//! ([`Crossing::set_map`]), and:
//!
//! * as the controller, the pointer was on the lost computer: the grab ends,
//!   the pointer is placed where it left this computer's display, and the
//!   shared mouse and keyboard act here again. Nothing can be sent to the
//!   lost computer, so what it holds down is released by the computer itself
//!   when it notices the loss; what is physically held on this computer's
//!   devices was swallowed by the grab and never reached a local application,
//!   so nothing here is left pressed.
//! * as the receiver, whatever the lost controller left pressed is released
//!   here and its input is no longer accepted.
//!
//! A connection replaced by a new one to the same computer (a re-dial after an
//! address change, Story 3 scenarios 1 and 3) does not lose it: the session
//! goes on over the new connection once that proves membership, and the
//! computer being controlled keeps the pointer.

use super::controller::Return;
use super::state::Pressed;
use super::{Crossing, DeskMap, edges, state};
use crate::logging::events::ControlHow;

impl Crossing {
    /// The computer being controlled is now reached over a new connection.
    /// What was sent over the old one may have been lost with it, so the
    /// session starts again, clean, where the pointer is: nothing stays
    /// pressed there that this computer no longer holds.
    pub(super) fn resume_on_new_link(&mut self, before: &DeskMap) {
        let state::Control::Remote(r) = &self.session.control else {
            return;
        };
        let (Some(old), Some(new), Some(to)) = (
            before.wires.get(&r.computer),
            self.map.wires.get(&r.computer).cloned(),
            edges::index_of(&self.map.placed, &r.display),
        ) else {
            return;
        };
        if old.same_link(&new) {
            return;
        }
        let (display, at) = (
            r.display.clone(),
            edges::to_local(&self.map.placed[to], r.at),
        );
        self.enter(&new, display, at);
        self.session.sent = Pressed::default();
    }

    /// Return control and stop receiving for every computer the current map
    /// no longer holds.
    pub(super) fn drop_lost(&mut self) {
        let controlled_gone = match &self.session.control {
            state::Control::Remote(r) => {
                let linked = self.map.wires.contains_key(&r.computer);
                let placed = edges::index_of(&self.map.placed, &r.display).is_some();
                (!linked || !placed).then_some(linked)
            }
            state::Control::Local => None,
        };
        // A computer still linked but without its display in the layout is
        // told to release what it was sent; an unreachable one cannot be.
        if let Some(linked) = controlled_gone {
            self.return_local(
                if linked {
                    Return::Release
                } else {
                    Return::Lost
                },
                ControlHow::LinkLost,
                None,
            );
        }
        if self.receiving.as_ref().is_some_and(|r| {
            !self.map.wires.contains_key(&r.from) || self.map.paused.contains(&r.from)
        }) {
            self.end_receiving();
        }
        self.drop_lost_letters();
    }
}
