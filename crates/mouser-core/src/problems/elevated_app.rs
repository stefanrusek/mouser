//! An application running as administrator is in front of portable Mouser
//! (Story 5 scenario 6): the pointer went back to the computer it came from
//! and that computer says why, naming the application and this computer.
//! The notice stays until control next enters this computer without such an
//! application in front.

use mouser_platform::ForegroundApp;
use mouser_proto::control::{FixAction, ProblemKind};
use mouser_proto::wire::ComputerMode;

use crate::engine::Ctx;
use crate::lock::LockExt;

impl Ctx {
    /// Hand control back if the application in front outranks Mouser, and
    /// explain it.
    pub(crate) fn hand_back_elevated(&self) {
        let Some(app) = self.crossing.guard().hand_back_elevated() else {
            return;
        };
        self.raise_elevated_app(&app);
    }

    fn raise_elevated_app(&self, app: &ForegroundApp) {
        let computer = self.display_name();
        let me = self.me();
        let (message, fix) = if self.mode == ComputerMode::Portable {
            (
                format!(
                    "Portable Mouser on {computer} can't control {} because it runs as \
                     administrator. Restart Mouser as administrator to control it, or install \
                     Mouser to also control the login screen, permission prompts and \
                     Ctrl-Alt-Del.",
                    app.name
                ),
                Some(FixAction::RestartElevated(me)),
            )
        } else {
            (
                format!(
                    "Mouser on {computer} can't control {} because it runs as administrator. \
                     Install Mouser to control it.",
                    app.name
                ),
                None,
            )
        };
        self.problems.raise_in(
            ProblemKind::ElevatedApp,
            &me.to_string(),
            me,
            message,
            Some(app.name.clone()),
            fix,
        );
    }
}
