//! The pointer on this computer: every grab and every return goes through
//! here, so the log says when it was hidden or shown and why (Story 2
//! scenario 3).

use super::Crossing;
use crate::logging::events::{self, InputKind, PointerHidden, PointerShown};

impl Crossing {
    /// Grab this computer's input, which hides the pointer. A grab the
    /// system refuses is logged as a refusal; where grabbing is not possible
    /// at all (`Unsupported`) nothing is hidden and nothing is said.
    pub(crate) fn grab(&mut self, reason: PointerHidden) -> mouser_platform::Result<()> {
        let io = self.io.as_mut().expect("input is running");
        let result = io.capture.set_grabbed(true);
        self.tally(InputKind::Grab, None, &result);
        if result.is_ok() && !std::mem::replace(&mut self.pointer_hidden, true) {
            self.log.write(events::pointer_hidden(reason));
        }
        result
    }

    /// Release the grab and show the pointer, and then say so. A computer
    /// whose input never started has nothing to ungrab.
    pub(super) fn ungrab(&mut self, reason: PointerShown) {
        let Some(io) = self.io.as_mut() else {
            return;
        };
        let result = io.capture.set_grabbed(false);
        self.tally(InputKind::Grab, None, &result);
        if result.is_ok() && std::mem::take(&mut self.pointer_hidden) {
            self.log.write(events::pointer_shown(reason));
        }
    }

    /// The backend lost this computer's input and has already ungrabbed and
    /// shown the pointer (platform rule 10): only the log has to hear of it.
    pub(super) fn pointer_lost(&mut self) {
        if std::mem::take(&mut self.pointer_hidden) {
            self.log
                .write(events::pointer_shown(PointerShown::CaptureLost));
        }
    }
}
