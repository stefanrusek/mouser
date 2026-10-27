//! The backend can no longer see this computer's input while the pointer is
//! on another computer (`LocalInput::CaptureLost`, research V7). It has
//! already ungrabbed and shown the pointer (platform rule 10), so control
//! comes back without touching the grab again, and the computer that had the
//! pointer is told to release what it was sent.

use mouser_proto::wire::ControlMessage;

use super::Crossing;
use super::state::{Control, Pressed};
use crate::logging::events::ControlHow;

impl Crossing {
    pub(super) fn on_capture_lost(&mut self) {
        let computer = self
            .session
            .remote()
            .expect("capture is only lost, as far as the input path is told, while remote");
        self.session.control = Control::Local;
        self.moved(computer, self.me, ControlHow::CaptureLost);
        self.pointer_lost();
        let seq = self.session.next_seq();
        // A computer that left the map has nothing to release.
        if let Some(wire) = self.map.wires.get(&computer) {
            wire.send(ControlMessage::ReleaseAll { seq });
        }
        self.session.sent = Pressed::default();
        self.dirty.notify_one();
    }
}
