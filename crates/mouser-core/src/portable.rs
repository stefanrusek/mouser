//! Portable behaviour (Story 13): a portable Mouser installs nothing on the
//! computer, so it never registers for start at login, and it says what it
//! cannot do on Windows (D3 item 4).

use mouser_platform::{Capability, Limitation, Os};

use crate::engine::{Ctx, EngineParams};
use crate::paths::Paths;

/// What portable mode cannot do on Windows.
pub const WINDOWS_LIMITATION: &str = "Portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.";

impl EngineParams {
    /// Keep everything in `paths`: its directory is the config directory and
    /// a portable Mouser keeps its secrets there too.
    pub fn with_paths(mut self, paths: &Paths) -> Self {
        self.config_dir = paths.dir.clone();
        self.secrets = paths.secrets();
        self.portable = paths.portable;
        self
    }
}

impl Ctx {
    /// Register for start at login, or stop, unless portable: then nothing is
    /// installed on this computer.
    pub(crate) fn set_autostart(&self, on: bool) {
        if !self.portable {
            self.platform
                .autostart()
                .set(on)
                .expect("autostart can be set");
        }
    }

    /// The platform's limitations and the ones portable mode adds.
    pub(crate) fn limitations(&self) -> Vec<Limitation> {
        let info = self.platform.info();
        let mut all = info.limitations;
        if self.portable && info.os == Os::Windows {
            all.push(Limitation {
                capability: Capability::SecureAttention,
                text: WINDOWS_LIMITATION.into(),
            });
        }
        all
    }
}
