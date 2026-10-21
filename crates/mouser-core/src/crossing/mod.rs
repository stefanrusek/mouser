//! Crossing and input forwarding: the pointer moves off the edge of one
//! computer's display onto another's, and the shared mouse and keyboard then
//! act there (Story 2).
//!
//! All crossing state lives in one [`Crossing`] behind a mutex of its own.
//! The input thread and the message handlers touch nothing else on the hot
//! path: peers are reached through cloned [`Wire`] handles (an unbounded queue
//! and datagram sends, neither of which blocks) and the layout through the
//! [`DeskMap`] snapshot, rebuilt by [`Ctx::refresh_map`] whenever it changes.

pub mod buttons;
pub mod controller;
pub mod corners;
pub mod fullscreen;
pub mod loss;
pub mod modifiers;
pub mod ordering;
pub mod r#override;
pub mod panic;
pub mod pause;
pub mod receiver;
pub mod state;

#[cfg(test)]
mod buttons_tests;
#[cfg(test)]
mod controller_tests;
#[cfg(test)]
mod corners_tests;
#[cfg(test)]
mod fullscreen_tests;
#[cfg(test)]
mod loss_tests;
#[cfg(test)]
mod modifiers_tests;
#[cfg(test)]
mod ordering_tests;
#[cfg(test)]
mod override_tests;
#[cfg(test)]
mod panic_tests;
#[cfg(test)]
mod pause_tests;
#[cfg(test)]
mod receiver_tests;
#[cfg(test)]
mod state_tests;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc;

use bytes::Bytes;
use mouser_platform::{
    Error, InputCapture, InputInject, LocalInput, PermissionKind, PermissionState, Platform,
};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, EdgeOptions, Os};
use mouser_proto::wire::{ControlMessage, MotionDatagram};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{Notify, broadcast};

use crate::clipboard::Command;
use crate::clipboard::targeted::{Chord, paste_chord};
use crate::engine::{Core, Ctx, EngineEvent, Outgoing, Phase};
use crate::layout::edges::{self, Placed};
use crate::problems::Problems;
use buttons::Buttons45;
use r#override::Bumps;
use panic::{PanicDetector, PanicSpec};
use receiver::Receiving;
use state::Session;

/// A connected, authenticated peer as the input path sees it.
#[derive(Clone)]
pub(crate) struct Wire {
    out: tokio::sync::mpsc::UnboundedSender<Outgoing>,
    conn: quinn::Connection,
    pub os: Os,
}

impl Wire {
    /// Queue a message on the reliable control stream.
    pub fn send(&self, msg: ControlMessage) {
        // A closed channel means the link already ended; its guard cleans up.
        let _ = self.out.send(Outgoing::Message(msg));
    }

    /// Send an unreliable motion datagram.
    pub fn datagram(&self, motion: &MotionDatagram) {
        let bytes = postcard::to_stdvec(motion).expect("datagram serializes");
        // Datagrams are unreliable by contract; a lost connection is noticed
        // by the link guard.
        let _ = self.conn.send_datagram(Bytes::from(bytes));
    }
}

/// The layout as the input path needs it.
#[derive(Default)]
pub(crate) struct DeskMap {
    /// Displays of this computer and of connected peers that have a placement.
    pub placed: Vec<Placed>,
    /// This computer's displays, placed or not.
    pub mine: Vec<Display>,
    /// Crossing maps whole edges onto whole edges (`equalize_dimensions`).
    pub equalize: bool,
    /// Per-edge options; an edge without an entry has the defaults.
    pub edge_options: Vec<EdgeOptions>,
    /// Full-screen blocking is on (`Layout.block_fullscreen`).
    pub block_fullscreen: bool,
    /// Authenticated peers.
    pub wires: BTreeMap<ComputerId, Wire>,
}

/// The platform's input: capture of the local devices and injection.
pub(crate) struct Io {
    pub capture: Box<dyn InputCapture>,
    pub inject: Box<dyn InputInject>,
}

pub(crate) struct Crossing {
    pub me: ComputerId,
    pub os: Os,
    pub buttons: Buttons45,
    pub paused: bool,
    pub map: DeskMap,
    pub io: Option<Io>,
    pub session: Session,
    pub panic: PanicDetector,
    pub receiving: Option<Receiving>,
    pub events: broadcast::Sender<EngineEvent>,
    /// Woken when what `status` reports about crossing changed.
    pub dirty: Arc<Notify>,
    pub platform: Arc<dyn Platform>,
    pub problems: Arc<Problems>,
    /// Blocked crossings that may be overridden by a second bump.
    pub bumps: Bumps,
    /// The targeted-paste hotkey.
    pub targeted_paste: Chord,
    /// The clipboard task, told of the paste shortcut and the hotkey.
    pub clipboard: UnboundedSender<Command>,
}

/// What the input path reports to and asks of the rest of the engine.
pub(crate) struct Host {
    pub events: broadcast::Sender<EngineEvent>,
    /// Woken when what `status` reports about crossing changed.
    pub dirty: Arc<Notify>,
    pub platform: Arc<dyn Platform>,
    pub problems: Arc<Problems>,
    pub clipboard: UnboundedSender<Command>,
}

impl Crossing {
    pub fn new(
        me: ComputerId,
        os: Os,
        buttons: Buttons45,
        panic: PanicSpec,
        targeted_paste: Chord,
        host: Host,
    ) -> Self {
        Self {
            me,
            os,
            buttons,
            paused: false,
            map: DeskMap::default(),
            io: None,
            session: Session::default(),
            panic: PanicDetector::new(panic),
            receiving: None,
            events: host.events,
            dirty: host.dirty,
            platform: host.platform,
            problems: host.problems,
            bumps: Bumps::default(),
            targeted_paste,
            clipboard: host.clipboard,
        }
    }

    /// A key went down while `held` were down, on this computer's keyboard
    /// or the controller's: the paste shortcut and the targeted-paste hotkey
    /// are clipboard business.
    pub fn clipboard_keys(&self, held: &std::collections::BTreeSet<u32>, usage: u32) {
        let command = if self.targeted_paste.pressed(held, usage) {
            Command::Targeted
        } else if paste_chord(self.os).pressed(held, usage) {
            Command::Paste
        } else {
            return;
        };
        // No clipboard task means this platform shares no clipboard.
        let _ = self.clipboard.send(command);
    }

    /// A new layout snapshot. Control that cannot continue on it (the
    /// computer or display went away) comes back here, and a controller that
    /// went away releases whatever it had pressed.
    pub fn set_map(&mut self, map: DeskMap) {
        self.map = map;
        self.drop_lost();
    }

    /// The computer input currently acts on, as far as this computer knows.
    pub fn active(&self) -> Option<ComputerId> {
        match (&self.session.control, &self.receiving) {
            (state::Control::Remote(r), _) => Some(r.computer),
            (state::Control::Local, Some(_)) => Some(self.me),
            (state::Control::Local, None) => None,
        }
    }

    /// Give control back and stop capturing.
    pub fn stop(&mut self) {
        self.return_local(controller::Return::Release, None);
        self.end_receiving();
        if let Some(mut io) = self.io.take() {
            io.capture.stop();
        }
    }
}

/// Permissions without which input can neither be captured nor injected.
const INPUT_PERMISSIONS: [PermissionKind; 4] = [
    PermissionKind::Accessibility,
    PermissionKind::InputMonitoring,
    PermissionKind::InputCapturePortal,
    PermissionKind::RemoteDesktopPortal,
];

/// `Err(Unsupported)` means the platform cannot do it (and says so in its
/// limitations); anything else is a failure.
pub(crate) fn supported<T>(result: mouser_platform::Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            assert_eq!(e, Error::Unsupported, "input is unavailable");
            None
        }
    }
}

impl Ctx {
    /// Rebuild the input path's view of the layout and peers.
    pub(crate) fn refresh_map(&self, core: &Core) {
        let mine = self.local_displays(core);
        let mut displays = mine.clone();
        let mut wires = BTreeMap::new();
        for (id, peer) in core.peers.iter().filter(|(_, p)| p.phase == Phase::Authed) {
            wires.insert(
                *id,
                Wire {
                    out: peer.out.clone(),
                    conn: peer.conn.clone(),
                    os: peer.hello.os,
                },
            );
            displays.extend(core.state.displays.get(id).cloned().unwrap_or_default());
        }
        let layout = core
            .state
            .desk
            .as_ref()
            .map(|d| d.state.layout.clone())
            .unwrap_or_default();
        let map = DeskMap {
            placed: edges::placed(&displays, &layout),
            equalize: layout.equalize_dimensions,
            edge_options: layout.edge_options.clone(),
            block_fullscreen: layout.block_fullscreen,
            mine,
            wires,
        };
        self.crossing.lock().unwrap().set_map(map);
    }

    /// Start capturing and injecting once the platform allows it; called at
    /// start and on every reconcile tick until it has started.
    pub(crate) fn try_start_input(self: &Arc<Self>) {
        if self.crossing.lock().unwrap().io.is_some() {
            return;
        }
        let blocked = self
            .platform
            .permissions()
            .into_iter()
            .any(|(kind, state)| {
                INPUT_PERMISSIONS.contains(&kind) && state == PermissionState::Missing
            });
        if blocked {
            return;
        }
        let (Some(mut capture), Some(inject)) = (
            supported(self.platform.capture()),
            supported(self.platform.inject()),
        ) else {
            return;
        };
        let (tx, rx) = mpsc::channel::<LocalInput>();
        capture.start(tx).expect("input capture starts");
        self.crossing.lock().unwrap().io = Some(Io { capture, inject });
        let crossing = self.crossing.clone();
        std::thread::Builder::new()
            .name("mouser-input".into())
            .spawn(move || {
                // Ends when the capture stops and drops its sender.
                for input in rx {
                    crossing
                        .lock()
                        .unwrap()
                        .local_input(input, std::time::Instant::now());
                }
            })
            .expect("input thread starts");
    }

    /// Return control, release everything and stop capturing.
    pub(crate) fn stop_input(&self) {
        self.crossing.lock().unwrap().stop();
    }
}

/// A motion datagram for `display` at `x`, `y`.
pub(crate) fn motion(
    seq: u64,
    display: &mouser_proto::model::DisplayId,
    x: f64,
    y: f64,
) -> MotionDatagram {
    MotionDatagram {
        seq,
        display_id_hash: mouser_proto::wire::display_id_hash(display),
        x: x as f32,
        y: y as f32,
    }
}
