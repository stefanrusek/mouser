//! Control returns to this computer when the computer that had it is lost
//! (Story 3 scenario 5, edge case "no stuck keys").
//!
//! A computer is lost when its link ends: a path silent for the 3 s idle
//! timeout, a peer that shut down, a connection replaced by a re-dial. The
//! layout snapshot then no longer has it ([`Crossing::set_map`]), and:
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

use super::controller::Return;
use super::{Crossing, edges, state};

impl Crossing {
    /// Return control and stop receiving for every computer the current map
    /// no longer holds.
    pub(super) fn drop_lost(&mut self) {
        let controlled_gone = match &self.session.control {
            state::Control::Remote(r) => {
                !self.map.wires.contains_key(&r.computer)
                    || edges::index_of(&self.map.placed, &r.display).is_none()
            }
            state::Control::Local => false,
        };
        if controlled_gone {
            self.return_local(Return::Lost, None);
        }
        if self
            .receiving
            .as_ref()
            .is_some_and(|r| !self.map.wires.contains_key(&r.from))
        {
            self.end_receiving();
        }
    }
}
