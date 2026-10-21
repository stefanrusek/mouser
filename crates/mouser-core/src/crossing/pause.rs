//! Pause and resume (Story 2 scenario 12): while paused this computer starts
//! no crossing, and control that is elsewhere comes back.

use super::Crossing;
use super::controller::Return;
use crate::engine::Ctx;

impl Crossing {
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.return_local(Return::Release, None);
        }
    }
}

impl Ctx {
    /// `active.pause` / `active.resume`.
    pub(crate) fn set_paused(&self, paused: bool) {
        let mut core = self.core.lock().unwrap();
        core.paused = paused;
        self.crossing.lock().unwrap().set_paused(paused);
        drop(core);
        self.notify_status();
    }
}
