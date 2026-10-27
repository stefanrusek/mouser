//! Who has the pointer (data-model "ActiveSession") and what is held down.

use std::collections::BTreeSet;

use mouser_platform::LocalInput;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;

/// Keys (HID usages) and mouse buttons that are down.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pressed {
    pub keys: BTreeSet<u32>,
    pub buttons: BTreeSet<u8>,
}

impl Pressed {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.buttons.is_empty()
    }
}

/// Where the shared mouse and keyboard act.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Control {
    /// On this computer.
    #[default]
    Local,
    Remote(Remote),
}

/// The pointer is on `computer`, at desk point `at` of its display `display`.
#[derive(Debug, Clone, PartialEq)]
pub struct Remote {
    pub computer: ComputerId,
    pub display: DisplayId,
    /// Desk-space position (millimetres).
    pub at: (f64, f64),
    /// Desk millimetres per device count, horizontally and vertically: the
    /// speed on every display is the speed of the display left.
    pub speed: (f64, f64),
    /// Where the local pointer returns to when control comes back without a
    /// crossing (panic, pause, lost connection).
    pub back: (f64, f64),
}

/// What leaving the current state must release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leave {
    /// Leaving this computer: what is physically held must be released here,
    /// because the grab swallows the physical releases.
    Local(Pressed),
    /// Leaving `computer`: it must release everything it was sent.
    Remote(ComputerId),
}

/// The controller's view of the session.
#[derive(Debug, Default)]
pub struct Session {
    pub control: Control,
    /// Physically held on this computer's devices.
    pub held: Pressed,
    /// Sent to the active computer and not yet released.
    pub sent: Pressed,
    /// Last sequence number used.
    pub seq: u64,
}

impl Session {
    /// Record a physical key or button change.
    pub fn track(&mut self, input: LocalInput) {
        match input {
            LocalInput::Key { usage, down } => set(&mut self.held.keys, usage, down),
            LocalInput::Button { button, down } => set(&mut self.held.buttons, button, down),
            LocalInput::Motion { .. } | LocalInput::Scroll { .. } => {}
            // v2: replaced in T111
            LocalInput::CaptureLost(_) => {}
        }
    }

    /// The next sequence number.
    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Leave the current state: everything it holds must be released, and
    /// nothing counts as sent any more.
    pub fn leave(&mut self) -> Leave {
        self.sent = Pressed::default();
        match &self.control {
            Control::Local => Leave::Local(self.held.clone()),
            Control::Remote(r) => Leave::Remote(r.computer),
        }
    }

    /// The pointer's state while it is on another computer.
    pub fn remote_mut(&mut self) -> Option<&mut Remote> {
        match &mut self.control {
            Control::Local => None,
            Control::Remote(r) => Some(r),
        }
    }

    /// The computer that has the pointer, when it is not this one.
    pub fn remote(&self) -> Option<ComputerId> {
        match &self.control {
            Control::Local => None,
            Control::Remote(r) => Some(r.computer),
        }
    }
}

fn set<T: Ord>(set: &mut BTreeSet<T>, item: T, down: bool) {
    if down {
        set.insert(item);
    } else {
        set.remove(&item);
    }
}
