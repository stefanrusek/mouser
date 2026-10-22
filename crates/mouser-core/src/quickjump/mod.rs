//! QuickJump (Story 11): one hotkey shows a letter on every display of the
//! desk; the letter's key moves the pointer to the centre of that display,
//! any other key puts the letters away.
//!
//! The computer whose keyboard the hotkey was pressed on is the controller:
//! it picks the letters, tells every desk-mate to show them, reads the key
//! that follows (from its own keyboard, whether the pointer is here or on
//! another computer) and tells everyone to hide them again.

#[cfg(test)]
mod tests;

use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;
use mouser_proto::wire::ControlMessage;

use crate::crossing::controller::Return;
use crate::crossing::panic::usage_for_name;
use crate::crossing::state::Control;
use crate::crossing::{Crossing, supported};
use crate::engine::EngineEvent;
use crate::layout::edges::{self, Placed};

/// The keys a display can be named by, in the order they are handed out.
const SYMBOLS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// A distinct letter for every display, left to right (then top to bottom)
/// on the desk.
pub fn assign(placed: &[Placed]) -> Vec<(DisplayId, char)> {
    assert!(
        placed.len() <= SYMBOLS.len(),
        "QuickJump has {} letters and the desk has {} displays",
        SYMBOLS.len(),
        placed.len()
    );
    let mut order: Vec<&Placed> = placed.iter().collect();
    order.sort_by_key(|p| (p.rect.x, p.rect.y));
    order
        .into_iter()
        .zip(SYMBOLS.chars())
        .map(|(p, letter)| (p.display.display_id.clone(), letter))
        .collect()
}

impl Crossing {
    /// A key went down on this computer's keyboard. True when QuickJump took
    /// it: the hotkey (starting QuickJump), or the key answering the letters.
    pub fn quickjump_key(&mut self, usage: u32) -> bool {
        if let Some(letters) = self.jump.take() {
            self.hide_letters();
            let chosen = letters
                .iter()
                .find(|(_, letter)| usage_for_name(&letter.to_string()) == Some(usage));
            // A display that left the layout since the letters were shown
            // cannot be jumped to: the jump is cancelled.
            if let Some((display, _)) = chosen
                && let Some(to) = edges::index_of(&self.map.placed, display)
            {
                self.jump_to(display, to);
            }
            true
        } else if !self.paused && self.quickjump.pressed(&self.session.held.keys, usage) {
            self.show_letters();
            true
        } else {
            false
        }
    }

    fn show_letters(&mut self) {
        let letters = assign(&self.map.placed);
        for wire in self.map.wires.values() {
            wire.send(ControlMessage::QuickJumpShow {
                letters: letters.clone(),
            });
        }
        // No subscribers is fine.
        let _ = self.events.send(EngineEvent::QuickJumpShow {
            letters: letters.clone(),
        });
        self.jump = Some(letters);
    }

    fn hide_letters(&self) {
        for wire in self.map.wires.values() {
            wire.send(ControlMessage::QuickJumpHide);
        }
        let _ = self.events.send(EngineEvent::QuickJumpHide);
    }

    /// The controller `from` shows letters: this computer's displays among
    /// them.
    pub fn on_quickjump_show(&mut self, from: ComputerId, letters: Vec<(DisplayId, char)>) {
        self.letters_from = Some(from);
        let _ = self.events.send(EngineEvent::QuickJumpShow { letters });
    }

    pub fn on_quickjump_hide(&mut self) {
        self.letters_from = None;
        let _ = self.events.send(EngineEvent::QuickJumpHide);
    }

    /// The computer that showed the letters is gone and will never hide them.
    pub(crate) fn drop_lost_letters(&mut self) {
        if self
            .letters_from
            .is_some_and(|from| !self.map.wires.contains_key(&from))
        {
            self.on_quickjump_hide();
        }
    }

    /// Move the pointer to the centre of `display`, and the shared keyboard
    /// with it.
    fn jump_to(&mut self, display: &DisplayId, to: usize) {
        let target = &self.map.placed[to];
        let (w, h) = target.display.logical_size;
        let at = edges::to_desk(target, (f64::from(w) / 2.0, f64::from(h) / 2.0));
        let owner = display.computer;
        if owner == self.me {
            let (x, y) = edges::to_local(target, at);
            let (ox, oy) = target.display.origin_local;
            let desktop = (f64::from(ox) + x, f64::from(oy) + y);
            match self.session.control {
                Control::Remote(_) => self.return_local(Return::Release, Some(desktop)),
                Control::Local => {
                    self.pointer = desktop;
                    self.io
                        .as_mut()
                        .expect("input is running")
                        .capture
                        .warp(desktop.0, desktop.1)
                        .expect("pointer can be placed");
                }
            }
        } else if self.session.remote() == Some(owner) {
            self.point_at(to, at);
        } else if !self.can_grab() {
            // Where input cannot be grabbed the pointer cannot be taken
            // there: the letters are put away and nothing else happens.
        } else {
            let (speed, back) = match &self.session.control {
                Control::Remote(r) => (r.speed, r.back),
                Control::Local => (edges::mm_per_px(self.pointer_display()), self.pointer),
            };
            self.release_current();
            self.begin(to, at, speed, back);
        }
    }

    /// Input can be grabbed here (it cannot where the platform lists the
    /// limitation, such as on Wayland).
    fn can_grab(&mut self) -> bool {
        let io = self.io.as_mut().expect("input is running");
        supported(io.capture.set_grabbed(true)).is_some()
    }

    /// The display of this computer the pointer is on (the first one when
    /// it is not known to be on any).
    fn pointer_display(&self) -> &Placed {
        let mine: Vec<&Placed> = self
            .map
            .placed
            .iter()
            .filter(|p| p.display.display_id.computer == self.me)
            .collect();
        let (px, py) = self.pointer;
        mine.iter()
            .find(|p| {
                let (ox, oy) = p.display.origin_local;
                let (w, h) = p.display.logical_size;
                (f64::from(ox)..f64::from(ox) + f64::from(w)).contains(&px)
                    && (f64::from(oy)..f64::from(oy) + f64::from(h)).contains(&py)
            })
            .copied()
            .unwrap_or(mine[0])
    }
}
