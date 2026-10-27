//! Recovery after a fault (research V3): whatever unit of work panicked, the
//! crossing state ends up `Local`, the pointer is shown and nothing is held.

use mouser_proto::wire::ControlMessage;

use super::Crossing;
use super::state::{Control, Pressed};
use crate::logging::events::{ControlHow, PointerShown};

impl Crossing {
    /// As controller: control comes back here, the grab ends (the pointer
    /// shows) and the computer that was driven is told to release what it
    /// was sent. As receiver: what was injected is released and the session
    /// is dropped.
    pub(crate) fn recover_after_fault(&mut self) {
        if let Control::Remote(r) = std::mem::take(&mut self.session.control) {
            let seq = self.session.next_seq();
            // A computer that left the map has nothing to release.
            if let Some(wire) = self.map.wires.get(&r.computer) {
                wire.send(ControlMessage::ReleaseAll { seq });
            }
            self.moved(r.computer, self.me, ControlHow::Fault);
        }
        self.session.sent = Pressed::default();
        self.ungrab(PointerShown::Fault);
        self.end_receiving();
        self.dirty.notify_one();
    }
}
