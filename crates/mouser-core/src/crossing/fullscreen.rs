//! Full-screen blocking (Story 5 scenario 2): while a full-screen
//! application is in front and `block_fullscreen` is on, the pointer does not
//! leave this computer, so a game or presentation is not interrupted.

use super::Crossing;

impl Crossing {
    /// A full-screen application is in front here and the desk blocks that.
    pub(super) fn fullscreen_blocks(&self) -> bool {
        self.map.block_fullscreen && self.platform.foreground_is_fullscreen()
    }
}
