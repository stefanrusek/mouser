//! The controller side: local input becomes a crossing, and while the pointer
//! is on another computer, motion goes as datagrams and keys, buttons and
//! scroll on the control stream, each with an increasing `seq`.

use std::time::Instant;

use mouser_platform::LocalInput;
use mouser_proto::wire::ControlMessage;

use super::Crossing;
use super::modifiers::held_modifiers;
use super::motion;
use super::state::{Control, Leave, Pressed, Remote};
use crate::layout::edges::{self, Step};
use crate::layout::equalize;

/// How control comes back to this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Return {
    /// A normal transfer: the computer left releases everything.
    Release,
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
        if self.session.remote().is_some() {
            self.forward(input, now);
        } else if let LocalInput::Motion { dx, dy, abs } = input {
            self.cross_from_local(dx, dy, abs, now);
        } else if let LocalInput::Key { usage, down: true } = input {
            self.clipboard_keys(&self.session.held.keys, usage);
        }
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
                self.begin(to, at, (mx, my), abs);
            }
            Step::Cross { .. } => {}
            _ => self.bumps.release(),
        }
    }

    /// Release what the state being left holds: the held keys and buttons
    /// here, or everything sent to the computer being left.
    fn release_current(&mut self) {
        match self.session.leave() {
            Leave::Local(held) => self.release_locally(&held),
            Leave::Remote(computer) => {
                let seq = self.session.next_seq();
                self.map.wires[&computer].send(ControlMessage::ReleaseAll { seq });
            }
        }
    }

    /// The grab swallows the physical releases of what is held now, so
    /// release it for local applications first.
    fn release_locally(&mut self, held: &Pressed) {
        let io = self.io.as_mut().expect("input is running");
        for usage in &held.keys {
            io.inject.key(*usage, false).expect("key can be released");
        }
        for button in &held.buttons {
            io.inject
                .button(*button, false)
                .expect("button can be released");
        }
    }

    /// Hand control to display `to`, entering at desk point `at`.
    fn begin(&mut self, to: usize, at: (f64, f64), speed: (f64, f64), back: (f64, f64)) {
        let target = &self.map.placed[to];
        let display = target.display.display_id.clone();
        let (x, y) = edges::to_local(target, at);
        let wire = self.map.wires[&display.computer].clone();
        self.io
            .as_mut()
            .expect("input is running")
            .capture
            .set_grabbed(true)
            .expect("input can be grabbed");
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
        self.panic.reset();
        self.session.control = Control::Remote(Remote {
            computer: display.computer,
            display,
            at,
            speed,
            back,
        });
        self.dirty.notify_one();
    }

    /// Give control back to this computer. `at` is where the pointer lands in
    /// desktop coordinates; without it, where it left from.
    pub fn return_local(&mut self, how: Return, at: Option<(f64, f64)>) {
        let Control::Remote(r) = std::mem::replace(&mut self.session.control, Control::Local)
        else {
            return;
        };
        match how {
            Return::Release => {
                let seq = self.session.next_seq();
                self.map.wires[&r.computer].send(ControlMessage::ReleaseAll { seq });
            }
            Return::Panic => self.map.wires[&r.computer].send(ControlMessage::Panic),
            Return::Lost => {}
        }
        self.session.sent = Pressed::default();
        let (x, y) = at.unwrap_or(r.back);
        let io = self.io.as_mut().expect("input is running");
        io.capture
            .set_grabbed(false)
            .expect("input can be released");
        io.capture.warp(x, y).expect("pointer can be placed");
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
            LocalInput::Key { usage, down } => {
                if down && self.panic.pressed(usage, now) {
                    self.return_local(Return::Panic, None);
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

    fn send(&self, msg: ControlMessage) {
        let computer = self.session.remote().expect("pointer is remote");
        self.map.wires[&computer].send(msg);
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
                        Some((f64::from(ox) + x, f64::from(oy) + y)),
                    );
                } else if owner == computer {
                    self.point_at(to, at);
                } else {
                    self.release_current();
                    self.begin(to, at, speed, back);
                }
            }
        }
    }

    /// The remote pointer is at desk point `at` of display `index`.
    fn point_at(&mut self, index: usize, at: (f64, f64)) {
        let target = &self.map.placed[index];
        let (x, y) = edges::to_local(target, at);
        let display = target.display.display_id.clone();
        let seq = self.session.next_seq();
        let datagram = motion(seq, &display, x, y);
        let r = self.session.remote_mut().expect("pointer is remote");
        r.display = display;
        r.at = at;
        self.map.wires[&r.computer].datagram(&datagram);
    }
}
