//! The receiver side: what a controller sends is injected into this
//! computer, in order, and input from this computer's own devices is left
//! alone.

use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DisplayId, Os};
use mouser_proto::wire::{MotionDatagram, display_id_hash};

use super::buttons::{self, Action};
use super::controller::Return;
use super::modifiers::map_usage;
use super::ordering::Sequencer;
use super::state::Pressed;
use super::{Crossing, Io};
use crate::engine::EngineEvent;

/// A controller has entered this computer.
#[derive(Debug)]
pub struct Receiving {
    pub from: ComputerId,
    pub os: Os,
    /// Motion datagrams: only the newest position matters, so older ones
    /// that arrive late are dropped.
    pub motion: Sequencer,
    /// Keys, buttons and scroll on the control stream: each must be newer
    /// than the one before.
    pub stream: Sequencer,
    /// Injected and not yet released.
    pub injected: Pressed,
}

impl Crossing {
    /// The controller we are receiving from, with the injector.
    pub(super) fn receiver(&mut self, from: ComputerId) -> Option<(&mut Receiving, &mut Io)> {
        match (self.receiving.as_mut(), self.io.as_mut()) {
            (Some(rx), Some(io)) if rx.from == from => Some((rx, io)),
            _ => None,
        }
    }

    /// `Enter`: the pointer arrives at `x`, `y` of one of our displays.
    pub fn on_enter(
        &mut self,
        from: ComputerId,
        display_id: DisplayId,
        x: f32,
        y: f32,
        seq_base: u64,
    ) -> Result<(), String> {
        // A paused computer is isolated: whoever crosses onto it is refused.
        if self.paused {
            return Ok(());
        }
        let os = self
            .map
            .wires
            .get(&from)
            .ok_or("Enter from a computer that is not on the desk")?
            .os;
        let origin = self
            .map
            .mine
            .iter()
            .find(|d| d.display_id == display_id)
            .ok_or_else(|| {
                format!("Enter on a display this computer does not have: {display_id:?}")
            })?
            .origin_local;
        self.end_receiving();
        // Another computer's own devices now drive this one, as when the
        // computer we were controlling crosses back onto us with its mouse:
        // our grab ends (showing our pointer again) and nothing more is
        // forwarded, as after the panic key.
        self.return_local(Return::Release, None);
        // A computer that cannot inject input cannot be controlled.
        let Some(io) = self.io.as_mut() else {
            return Ok(());
        };
        let desktop = (
            f64::from(origin.0) + f64::from(x),
            f64::from(origin.1) + f64::from(y),
        );
        io.inject
            .motion_abs(desktop.0, desktop.1)
            .expect("pointer can be moved");
        self.receiving = Some(Receiving {
            from,
            os,
            motion: Sequencer::after(seq_base),
            stream: Sequencer::after(seq_base),
            injected: Pressed::default(),
        });
        // No subscribers is fine.
        let _ = self.events.send(EngineEvent::Landing {
            display: display_id,
            x,
            y,
            desktop,
        });
        self.dirty.notify_one();
        Ok(())
    }

    /// A motion datagram; stale ones (older `seq`) and ones for a display we
    /// no longer have are dropped.
    pub fn on_motion(&mut self, from: ComputerId, motion: MotionDatagram) {
        let Some(origin) = self
            .map
            .mine
            .iter()
            .find(|d| display_id_hash(&d.display_id) == motion.display_id_hash)
            .map(|d| d.origin_local)
        else {
            return;
        };
        let Some((rx, io)) = self.receiver(from) else {
            return;
        };
        if !rx.motion.admit(motion.seq) {
            return;
        }
        io.inject
            .motion_abs(
                f64::from(origin.0) + f64::from(motion.x),
                f64::from(origin.1) + f64::from(motion.y),
            )
            .expect("pointer can be moved");
    }

    /// A key, translated for this computer's keyboard shortcuts.
    pub fn on_key(&mut self, from: ComputerId, seq: u64, usage: u32, down: bool) {
        let me = self.os;
        let Some((rx, io)) = self.receiver(from) else {
            return;
        };
        if !rx.stream.admit(seq) {
            return;
        }
        let usage = map_usage(rx.os, me, usage);
        press(&mut rx.injected, io, Action::Key(usage, down));
        if down {
            let held = rx.injected.keys.clone();
            self.clipboard_keys(&held, usage);
        }
    }

    /// A mouse button, with buttons 4 and 5 as configured.
    pub fn on_button(&mut self, from: ComputerId, seq: u64, button: u8, down: bool) {
        let (setting, os) = (self.buttons, self.os);
        let Some((rx, io)) = self.receiver(from) else {
            return;
        };
        if !rx.stream.admit(seq) {
            return;
        }
        for action in buttons::actions(setting, os, button, down) {
            press(&mut rx.injected, io, action);
        }
    }

    pub fn on_scroll(&mut self, from: ComputerId, seq: u64, dx: i32, dy: i32) {
        let Some((rx, io)) = self.receiver(from) else {
            return;
        };
        if !rx.stream.admit(seq) {
            return;
        }
        io.inject
            .scroll(f64::from(dx), f64::from(dy))
            .expect("scroll can be injected");
    }

    /// `ReleaseAll` and `Panic`: release everything and take no more input
    /// until the next `Enter`.
    pub fn on_release_all(&mut self, from: ComputerId) {
        if self.receiver(from).is_some() {
            self.end_receiving();
        }
    }

    /// Release what the controller left pressed and stop receiving.
    pub fn end_receiving(&mut self) {
        let Some(rx) = self.receiving.take() else {
            return;
        };
        let io = self.io.as_mut().expect("receiving implies input");
        for usage in rx.injected.keys {
            io.inject.key(usage, false).expect("key can be released");
        }
        for button in rx.injected.buttons {
            io.inject
                .button(button, false)
                .expect("button can be released");
        }
        self.dirty.notify_one();
    }
}

/// Inject one action and remember what stays pressed.
fn press(injected: &mut Pressed, io: &mut Io, action: Action) {
    match action {
        Action::Key(usage, down) => {
            io.inject.key(usage, down).expect("key can be injected");
            set(&mut injected.keys, usage, down);
        }
        Action::Button(button, down) => {
            io.inject
                .button(button, down)
                .expect("button can be injected");
            set(&mut injected.buttons, button, down);
        }
    }
}

fn set<T: Ord>(set: &mut std::collections::BTreeSet<T>, item: T, down: bool) {
    if down {
        set.insert(item);
    } else {
        set.remove(&item);
    }
}
