//! Permission problems (Story 8 scenario 2) and the fix actions the app
//! invokes with one click.

use mouser_platform::PermissionState as PlatformState;
use mouser_proto::control::{FixAction, ProblemKind};

use super::permission_text;
use crate::control::status::permission_kind;
use crate::engine::Ctx;

impl Ctx {
    /// Raise a `missing_permission` problem (with the fix that opens the right
    /// system screen) for each missing permission, and clear it once granted.
    /// Returns whether anything changed.
    pub(crate) fn sync_permission_problems(&self) -> bool {
        let mut changed = false;
        for (kind, state) in self.platform.permissions() {
            let wire = permission_kind(kind);
            let subject = serde_json::to_value(wire).expect("kind serializes");
            let subject = subject.as_str().expect("kind is a string");
            changed |= if state == PlatformState::Missing {
                self.problems.raise(
                    ProblemKind::MissingPermission,
                    subject,
                    self.me(),
                    permission_text(wire).to_string(),
                    Some(FixAction::OpenSystemSettings(wire)),
                )
            } else {
                self.problems
                    .clear(ProblemKind::MissingPermission, subject, self.me())
            };
        }
        changed
    }

    /// Carry out the fix of the open problem `problem_id` of this computer
    /// (what a desk-mate reports is fixed on that computer). Opening a system
    /// screen is done here; `open_arranger_edge` only needs the problem
    /// acknowledged (the app opens the arranger), which clears it. The other
    /// fixes are the app's own.
    pub(crate) fn apply_fix(&self, problem_id: &str) -> Result<(), String> {
        let problem = self
            .problems
            .open()
            .into_iter()
            .find(|p| p.problem_id == problem_id)
            .ok_or_else(|| format!("{problem_id} is not an open problem"))?;
        match problem.fix {
            Some(FixAction::OpenSystemSettings(kind)) => {
                self.platform
                    .open_permission_settings(platform_kind(kind))
                    .expect("the system settings open");
                Ok(())
            }
            Some(FixAction::OpenArrangerEdge) => {
                self.problems.clear_id(problem_id, problem.computer);
                Ok(())
            }
            _ => Err(format!("{problem_id} is fixed by the app, not the engine")),
        }
    }
}

fn platform_kind(kind: mouser_proto::model::PermissionKind) -> mouser_platform::PermissionKind {
    use mouser_platform::PermissionKind as P;
    use mouser_proto::model::PermissionKind as W;
    match kind {
        W::Accessibility => P::Accessibility,
        W::InputMonitoring => P::InputMonitoring,
        W::ScreenRecording => P::ScreenRecording,
        W::FullDiskAccess => P::FullDiskAccess,
        W::InputCapturePortal => P::InputCapturePortal,
        W::RemoteDesktopPortal => P::RemoteDesktopPortal,
    }
}
