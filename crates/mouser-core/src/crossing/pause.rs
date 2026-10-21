//! Pause and resume.

use crate::engine::Ctx;

impl Ctx {
    /// `active.pause` / `active.resume`.
    pub(crate) fn set_paused(&self, paused: bool) {
        self.core.lock().unwrap().paused = paused;
        self.notify_status();
    }
}
