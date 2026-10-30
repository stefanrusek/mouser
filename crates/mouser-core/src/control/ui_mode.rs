//! What an app in UI mode (Windows installed) needs from the control API
//! beyond the 0.2 methods: `problem.fix`, `transfer.resolve` and the
//! `engine_event` topic, mirroring the in-process [`super::ControlHandle`].

use mouser_proto::control::EngineEvent as Wire;
use mouser_proto::control::{ProblemFixParams, RpcError, TransferResolveParams};

use crate::control::invalid;
use crate::engine::{Ctx, EngineEvent};

impl Ctx {
    /// `problem.fix`: carry out the fix of the open problem
    /// `(problem_id, computer)`, as [`super::ControlHandle::fix`] does.
    pub(crate) fn problem_fix(&self, p: ProblemFixParams) -> Result<(), RpcError> {
        let open = self
            .problems
            .open()
            .into_iter()
            .any(|o| o.problem_id == p.problem_id && o.computer == p.computer);
        if !open {
            return Err(invalid(format!("{} is not an open problem", p.problem_id)));
        }
        self.fix(&p.problem_id).map_err(invalid)
    }

    /// Carry out the fix of the open problem `problem_id` and announce the
    /// new status.
    pub(crate) fn fix(&self, problem_id: &str) -> Result<(), String> {
        self.apply_fix(problem_id)?;
        self.notify_status();
        Ok(())
    }

    /// `transfer.resolve`: answer the question a `transfer` notification in
    /// state `awaiting_overwrite_decision` asked.
    pub(crate) fn resolve_transfer(&self, p: TransferResolveParams) {
        // No transfer task means this platform cannot drag files, and then
        // there is no question to answer.
        let _ = self.xfer.send(crate::transfer::engine::Command::Resolve {
            transfer_id: p.transfer_id,
            overwrite: p.overwrite,
        });
    }
}

/// The `engine_event` form of `event`, for the events an app in UI mode
/// shows (a recovered fault is not one).
pub(crate) fn ui_event(event: EngineEvent) -> Option<Wire> {
    Some(match event {
        EngineEvent::Landing {
            display,
            x,
            y,
            desktop,
        } => Wire::Landing {
            display,
            x,
            y,
            desktop,
        },
        EngineEvent::ReviewOffered { computer, name } => Wire::ReviewOffered { computer, name },
        EngineEvent::QuickJumpShow { letters } => Wire::QuickJumpShow { letters },
        EngineEvent::QuickJumpHide => Wire::QuickJumpHide,
        EngineEvent::RestartElevated => Wire::RestartElevated,
        EngineEvent::Fault { .. } => return None,
    })
}
