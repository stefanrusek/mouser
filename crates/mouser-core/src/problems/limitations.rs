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

/// FNV-1a (64-bit): a deterministic hash, stable across runs and versions.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

impl Ctx {
    /// Raise a `platform_limitation` problem for each limitation the
    /// platform reports. They are facts about the platform, so once at start.
    pub(crate) fn raise_limitations(&self) {
        for limitation in self.limitations() {
            self.problems.raise(
                ProblemKind::PlatformLimitation,
                slug(limitation.capability),
                self.me(),
                limitation.text,
                None,
            );
        }
    }

    /// Raise a problem the platform noticed by itself while running.
    pub(crate) fn raise_platform_problem(&self, text: String) {
        // Problems de-duplicate by id, so each distinct text gets its own.
        let subject = format!("platform_problem:{:016x}", fnv1a(&text));
        self.problems.raise(
            ProblemKind::PlatformLimitation,
            &subject,
            self.me(),
            text,
            None,
        );
    }
}
