//! Platform limitations (FR-001): every limitation the platform reports is
//! shown as a problem in plain language.

use mouser_platform::Capability;
use mouser_proto::control::ProblemKind;

use crate::engine::Ctx;

fn slug(capability: Capability) -> &'static str {
    match capability {
        Capability::Capture => "capture",
        Capability::Inject => "inject",
        Capability::Clipboard => "clipboard",
        Capability::FileDrag => "file_drag",
        Capability::SecureAttention => "secure_attention",
    }
}

impl Ctx {
    /// Raise a `platform_limitation` problem for each limitation the
    /// platform reports. They are facts about the platform, so once at start.
    pub(crate) fn raise_limitations(&self) {
        for limitation in self.platform.info().limitations {
            self.problems.raise(
                ProblemKind::PlatformLimitation,
                slug(limitation.capability),
                self.me(),
                limitation.text,
                None,
            );
        }
    }
}
