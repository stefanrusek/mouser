//! `elevation.restart` (Story 5 scenario 6): restart a portable Windows
//! Mouser as administrator. For this computer the app is asked to; for a
//! desk-mate that is at least 0.3.0 it is sent `RestartElevated`.

use mouser_platform::Os;
use mouser_proto::control::{ElevationRestartParams, ErrorData, ProblemKind, RpcError};
use mouser_proto::wire::ComputerMode;
use serde_json::{Value, json};

use crate::engine::{Ctx, EngineEvent, Phase};
use crate::lock::LockExt;
use mouser_proto::wire::ControlMessage;

impl Ctx {
    /// This computer is Windows running portable and not elevated.
    fn restartable(&self) -> bool {
        self.platform.info().os == Os::Windows && self.mode == ComputerMode::Portable
    }

    pub(crate) fn elevation_restart(
        &self,
        params: ElevationRestartParams,
    ) -> Result<Value, RpcError> {
        let computer = params.computer;
        if computer == self.me() {
            if !self.restartable() {
                return Err(self.cannot_restart());
            }
            // No subscribers is fine.
            let _ = self.events.send(EngineEvent::RestartElevated);
            return Ok(json!({}));
        }
        let core = self.core.guard();
        let peer = core
            .peers
            .get(&computer)
            .filter(|p| p.phase == Phase::Authed)
            .filter(|p| {
                p.hello.os == mouser_proto::model::Os::Windows
                    && p.hello.speaks_v2()
                    && p.status.as_ref().map(|s| s.mode) == Some(ComputerMode::Portable)
            });
        match peer {
            Some(peer) => {
                peer.send(ControlMessage::RestartElevated);
                Ok(json!({}))
            }
            None => {
                drop(core);
                Err(self.cannot_restart())
            }
        }
    }

    /// A computer that is already elevated, installed, not Windows, too old
    /// or not on the desk cannot be restarted as administrator by Mouser.
    fn cannot_restart(&self) -> RpcError {
        RpcError {
            code: -32001,
            message:
                "Only portable Mouser 0.3.0 or newer on Windows can be restarted as administrator."
                    .into(),
            data: Some(ErrorData {
                kind: ProblemKind::PlatformLimitation,
            }),
        }
    }

    /// A desk-mate asks this computer to restart as administrator.
    pub(crate) fn on_restart_elevated(&self) -> Result<(), String> {
        if self.restartable() {
            // No subscribers is fine.
            let _ = self.events.send(EngineEvent::RestartElevated);
        }
        Ok(())
    }
}
