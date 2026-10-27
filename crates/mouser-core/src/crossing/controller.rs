//! The controller side: local input becomes a crossing, and while the pointer
//! is on another computer, motion goes as datagrams and keys, buttons and
//! scroll on the control stream, each with an increasing `seq`.

use std::time::Instant;

use mouser_platform::{Error, LocalInput};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;

use super::Crossing;
use super::modifiers::held_modifiers;
use super::motion;
use super::state::{Control, Leave, Pressed, Remote};
use crate::layout::edges::{self, Step};
use crate::layout::equalize;
use crate::logging::events::{ControlHow, InputKind, PointerHidden, PointerShown};

/// How control comes back to this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Return {
    /// A normal transfer: the computer left releases everything.
    Release,
    /// This computer's Mouser is quitting: as a transfer, and the engine
    /// then closes the links.
    Quit,
    /// The panic key: the computer left releases everything and stops
    /// accepting input.
    Panic,
    /// The computer is gone; there is no one to tell.
    Lost,
}

impl Crossing {
    /// One event from this computer's own mouse or keyboard.
    pub fn local_input(&mut self, input: LocalInput, now: Instant) {
        self.session.track(input);
        if let LocalInput::Key { usage, down: true } = input
            && self.quickjump_key(usage)
        {
            return;
        }
        if self.session.remote().is_some() {
            self.forward(input, now);
        } else if let LocalInput::Motion { dx, dy, abs } = input {
            self.pointer = abs;
            self.cross_from_local(dx, dy, abs, now);
        } else if let LocalInput::Key { usage, down: true } = input {
            if self.panic.pressed(usage, now) {
                self.panic_desk();
            }
            self.clipboard_keys(&self.session.held.keys, usage);
        }
    }

    /// The panic key, pressed on this computer's own keyboard: every computer
    /// on the desk goes back to its own devices. Control here comes back and
    /// what a controller sent here is released, and every desk-mate is told
    /// with `Panic` to do the same.
    fn panic_desk(&mut self) {
        let controlled = self.session.remote();
        self.return_local(Return::Panic, ControlHow::Panic, None);
        self.end_receiving();
        for (computer, wire) in &self.map.wires {
            if Some(*computer) != controlled {
                wire.send(ControlMessage::Panic);
            }
        }
    }

    /// `Panic` from `from`: the panic key was pressed on a desk-mate. What it
    /// sent here is released and control that is elsewhere comes back here.
    pub fn on_panic(&mut self, from: ComputerId) {
        self.on_release_all(from);
        self.return_local(Return::Release, ControlHow::Panic, None);
    }

    /// The pointer pushes past the edge of one of this computer's displays.
    fn cross_from_local(&mut self, dx: f64, dy: f64, abs: (f64, f64), now: Instant) {
        if self.paused {
            return;
        }
        let Some(here) = self.map.placed.iter().position(|p| {
            let (ox, oy) = p.display.origin_local;
            let (w, h) = p.display.logical_size;
            p.display.display_id.computer == self.me
                && (f64::from(ox)..=f64::from(ox) + f64::from(w)).contains(&abs.0)
                && (f64::from(oy)..=f64::from(oy) + f64::from(h)).contains(&abs.1)
        }) else {
            return;
        };
        let source = &self.map.placed[here];
        let (ox, oy) = source.display.origin_local;
        let wanted = edges::to_desk(
            source,
            (abs.0 + dx - f64::from(ox), abs.1 + dy - f64::from(oy)),
        );
        let (mx, my) = edges::mm_per_px(source);
        match equalize::step(&self.map.placed, here, wanted, self.map.equalize) {
            Step::Cross { to, at }
                if self.map.placed[to].display.display_id.computer != self.me
                    && self.may_cross(here, wanted, true, now) =>
            {
                self.release_current();
                self.begin(to, at, (mx, my), abs, ControlHow::Crossing);
            }
            Step::Cross { .. } => {}
            _ => self.bumps.release(),
        }
    }

    /// Release what the state being left holds: the held keys and buttons
    /// here, or everything sent to the computer being left.
    pub(crate) fn release_current(&mut self) {
        match self.session.leave() {
            Leave::Local(held) => self.release_locally(&held),
            Leave::Remote(computer) => {
                let seq = self.session.next_seq();
                // A computer that left the map has nothing to release.
                if let Some(wire) = self.map.wires.get(&computer) {
                    wire.send(ControlMessage::ReleaseAll { seq });
                }
            }
        }
    }

    /// The grab swallows the physical releases of what is held now, so
    /// release it for local applications first.
    fn release_locally(&mut self, held: &Pressed) {
        let io = self.io.as_mut().expect("input is running");
        // A refused release is skipped.
        for usage in &held.keys {
            let _ = io.inject.key(*usage, false);
        }
        for button in &held.buttons {
            let _ = io.inject.button(*button, false);
        }
    }

    /// Hand control to display `to`, entering at desk point `at`. Nothing
    /// happens when the computer is no longer on the map or the system
    /// refuses the grab.
    pub(crate) fn begin(
        &mut self,
        to: usize,
        at: (f64, f64),
        speed: (f64, f64),
        back: (f64, f64),
        how: ControlHow,
    ) {
        let target = &self.map.placed[to];
        let display = target.display.display_id.clone();
        let (x, y) = edges::to_local(target, at);
        let Some(wire) = self.map.wires.get(&display.computer).cloned() else {
            return;
        };
        // Where input cannot be grabbed (Wayland, `Unsupported`) the
        // platform's capture is driven by the screen edge instead; a grab the
        // system refuses means the pointer stays here.
        let reason = match how {
            ControlHow::Quickjump => PointerHidden::Quickjump,
            _ => PointerHidden::CrossingOut,
        };
        if let Err(Error::Os(_)) = self.grab(reason) {
            return;
        }
        let from = self.session.remote().unwrap_or(self.me);
        // This computer's own devices take over: whoever was driving it stops
        // being obeyed (what it held is released here).
        self.end_receiving();
        // A computer that kept something pressed from before starts clean.
        let seq = self.session.next_seq();
        wire.send(ControlMessage::ReleaseAll { seq });
        wire.send(ControlMessage::Enter {
            display_id: display.clone(),
            x: x as f32,
            y: y as f32,
            seq_base: seq,
            modifiers_down: held_modifiers(&self.session.held.keys),
        });
        self.hand_off_drag(display.computer);
        self.panic.reset();
        let computer = display.computer;
        self.session.control = Control::Remote(Remote {
            computer,
            display,
            at,
            speed,
            back,
        });
        self.moved(from, computer, how);
        self.dirty.notify_one();
    }

    /// Give control back to this computer. `at` is where the pointer lands in
    /// desktop coordinates; without it, where it left from.
    pub fn return_local(&mut self, how: Return, why: ControlHow, at: Option<(f64, f64)>) {
        let Control::Remote(r) = std::mem::replace(&mut self.session.control, Control::Local)
        else {
            return;
        };
        match how {
            Return::Release | Return::Quit => {
                let seq = self.session.next_seq();
                if let Some(wire) = self.map.wires.get(&r.computer) {
                    wire.send(ControlMessage::ReleaseAll { seq });
                }
            }
            Return::Panic => {
                if let Some(wire) = self.map.wires.get(&r.computer) {
                    wire.send(ControlMessage::Panic);
                }
            }
            Return::Lost => {}
        }
        self.session.sent = Pressed::default();
        let (x, y) = at.unwrap_or(r.back);
        self.moved(r.computer, self.me, why);
        // A refusal leaves nothing more to do: control is back regardless.
        self.ungrab(if how == Return::Quit {
            PointerShown::Quit
        } else {
            PointerShown::Returned
        });
        let io = self.io.as_mut().expect("input is running");
        let warped = io.capture.warp(x, y);
        self.tally(InputKind::Warp, None, &warped);
        self.dirty.notify_one();
    }

    /// The pointer is on another computer: send it what the devices do.
    fn forward(&mut self, input: LocalInput, now: Instant) {
        match input {
            LocalInput::Motion { dx, dy, .. } => self.move_remote(dx, dy, now),
            LocalInput::Button { button, down } => {
                if self.was_sent(
                    down,
                    |s| s.buttons.insert(button),
                    |s| s.buttons.remove(&button),
                ) {
                    let seq = self.session.next_seq();
                    self.send(ControlMessage::Button { seq, button, down });
                }
            }
            LocalInput::Scroll { dx, dy } => {
                let seq = self.session.next_seq();
                self.send(ControlMessage::Scroll {
                    seq,
                    dx: dx.round() as i32,
                    dy: dy.round() as i32,
                });
            }
            LocalInput::CaptureLost(_) => self.on_capture_lost(),
            LocalInput::Key { usage, down } => {
                if down && self.panic.pressed(usage, now) {
                    self.panic_desk();
                } else if self.was_sent(down, |s| s.keys.insert(usage), |s| s.keys.remove(&usage)) {
                    let seq = self.session.next_seq();
                    let modifiers = held_modifiers(&self.session.held.keys);
                    self.send(ControlMessage::Key {
                        seq,
                        usage,
                        down,
                        modifiers,
                    });
                }
            }
        }
    }

    /// Record a press as sent; a release counts only if its press was sent
    /// (one held across the crossing was released locally instead).
    fn was_sent(
        &mut self,
        down: bool,
        press: impl FnOnce(&mut Pressed) -> bool,
        release: impl FnOnce(&mut Pressed) -> bool,
    ) -> bool {
        if down {
            press(&mut self.session.sent);
            true
        } else {
            release(&mut self.session.sent)
        }
    }

    /// Send to the computer that has the pointer. One that left the map is
    /// lost: control returns here.
    fn send(&mut self, msg: ControlMessage) {
        let computer = self.session.remote().expect("pointer is remote");
        match self.map.wires.get(&computer) {
            Some(wire) => wire.send(msg),
            None => self.return_local(Return::Lost, ControlHow::LinkLost, None),
        }
    }

    /// Move the remote pointer by device deltas, crossing onward when it
    /// leaves its display.
    fn move_remote(&mut self, dx: f64, dy: f64, now: Instant) {
        let r = self
            .session
            .remote_mut()
            .expect("pointer is remote")
            .clone();
        let (computer, speed, back) = (r.computer, r.speed, r.back);
        let from = edges::index_of(&self.map.placed, &r.display).expect("display is on the map");
        let wanted = (r.at.0 + dx * speed.0, r.at.1 + dy * speed.1);
        match equalize::step(&self.map.placed, from, wanted, self.map.equalize) {
            Step::Stay => {
                self.bumps.release();
                self.point_at(from, wanted);
            }
            Step::Clamp { at } => {
                self.bumps.release();
                self.point_at(from, at);
            }
            Step::Cross { .. } if !self.may_cross(from, wanted, false, now) => {
                let held = edges::clamp_into(self.map.placed[from].rect, wanted);
                self.point_at(from, held);
            }
            Step::Cross { to, at } => {
                let target = &self.map.placed[to];
                let owner = target.display.display_id.computer;
                if owner == self.me {
                    let (x, y) = edges::to_local(target, at);
                    let (ox, oy) = target.display.origin_local;
                    self.return_local(
                        Return::Release,
                        ControlHow::Crossing,
                        Some((f64::from(ox) + x, f64::from(oy) + y)),
                    );
                } else if owner == computer {
                    self.point_at(to, at);
                } else {
                    self.release_current();
                    self.begin(to, at, speed, back, ControlHow::Crossing);
                }
            }
        }
    }

    /// The remote pointer is at desk point `at` of display `index`.
    pub(crate) fn point_at(&mut self, index: usize, at: (f64, f64)) {
        let target = &self.map.placed[index];
        let (x, y) = edges::to_local(target, at);
        let display = target.display.display_id.clone();
        let seq = self.session.next_seq();
        let datagram = motion(seq, &display, x, y);
        let r = self.session.remote_mut().expect("pointer is remote");
        r.display = display;
        r.at = at;
        let computer = r.computer;
        match self.map.wires.get(&computer) {
            Some(wire) => wire.datagram(&datagram),
            None => self.return_local(Return::Lost, ControlHow::LinkLost, None),
        }
    }
}
