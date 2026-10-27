//! Send Ctrl-Alt-Del (Story 12 scenario 4): the secure attention sequence
//! cannot be typed through ordinary input injection, so the controller asks
//! the active computer's platform to signal it.

use crate::lock::LockExt;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::Os;
use mouser_proto::wire::ControlMessage;

use mouser_proto::control::{ErrorData, ProblemKind, RpcError};

use super::{Crossing, supported};
use crate::engine::Ctx;

impl Crossing {
    /// Send the secure attention sequence to the active computer: the one
    /// the pointer is on, or this one. `Err` holds the computer that cannot
    /// receive it (it is not Windows, or its input is not running).
    pub fn send_cad(&mut self) -> Result<(), ComputerId> {
        match self.session.remote() {
            Some(computer) => {
                let wire = &self.map.wires[&computer];
                if wire.os != Os::Windows {
                    return Err(computer);
                }
                wire.send(ControlMessage::SecureAttention);
                Ok(())
            }
            None => {
                let me = self.me;
                let io = self.io.as_mut().filter(|_| self.os == Os::Windows);
                let io = io.ok_or(me)?;
                supported(io.inject.secure_attention()).ok_or(me)
            }
        }
    }

    /// `controller` asks for the secure attention sequence on this computer.
    /// Only the computer that has the pointer here may.
    pub fn on_secure_attention(&mut self, controller: ComputerId) {
        let Some((_, io)) = self.receiver(controller) else {
            return;
        };
        // A platform that cannot says so in its limitations.
        let _ = supported(io.inject.secure_attention());
    }
}

impl Ctx {
    /// The control API's `send_cad`.
    pub(crate) fn send_cad(&self) -> Result<(), RpcError> {
        let core = self.core.guard();
        let result = self.crossing.guard().send_cad();
        result.map_err(|computer| {
            let name = match computer == self.me() {
                true => self.display_name_in(&core),
                false => core.name_of(computer),
            };
            RpcError {
                code: -32001,
                message: format!("Ctrl-Alt-Del cannot be sent to {name}."),
                data: Some(ErrorData {
                    kind: ProblemKind::PlatformLimitation,
                }),
            }
        })
    }
}
