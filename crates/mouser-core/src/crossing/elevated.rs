//! The receiver hands control back when the application in front outranks
//! Mouser (Story 5 scenario 6, research V6): the operating system would
//! refuse every input Mouser injects into it, so the computer that has the
//! pointer is sent `Panic` (which 0.2.6 and later understand) and gets it
//! back, and this computer says why (`problems::elevated_app`).

use mouser_platform::ForegroundApp;
use mouser_proto::control::ProblemKind;
use mouser_proto::wire::ControlMessage;

use super::Crossing;
use crate::logging::events::{self, ControlHow};

impl Crossing {
    /// While receiving from a computer, called on each reconcile tick: if
    /// the application in front outranks Mouser, release what is held, send
    /// `Panic` to that computer only and say so in the log. Returns the
    /// application.
    pub(crate) fn hand_back_elevated(&mut self) -> Option<ForegroundApp> {
        let from = self.receiving.as_ref()?.from;
        let app = self
            .platform
            .foreground_app()
            .filter(|app| app.outranks_mouser)?;
        self.end_receiving();
        self.map.wires[&from].send(ControlMessage::Panic);
        self.log.write(events::control_changed(
            self.me,
            from,
            ControlHow::ElevatedApp,
            None,
        ));
        Some(app)
    }

    /// Control entered this computer: the explanation of an earlier
    /// hand-back is out of date unless an application outranking Mouser is
    /// in front again.
    pub(super) fn clear_elevated_app(&self) {
        if !self
            .platform
            .foreground_app()
            .is_some_and(|app| app.outranks_mouser)
        {
            self.problems
                .clear(ProblemKind::ElevatedApp, &self.me.to_string(), self.me);
        }
    }
}
