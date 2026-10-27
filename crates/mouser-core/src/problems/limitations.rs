//! Platform limitations (FR-010): the ones the platform reports at start-up
//! are listed on their computer (`status.computers[].limitations`) and never
//! raised as problems. What the platform notices while running is a problem.

use mouser_proto::control::ProblemKind;

use crate::engine::Ctx;

/// FNV-1a (64-bit): a deterministic hash, stable across runs and versions.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn platform_problem_subject(text: &str) -> String {
    format!("platform_problem:{:016x}", fnv1a(text))
}

impl Ctx {
    /// Raise a problem the platform noticed by itself while running.
    pub(crate) fn raise_platform_problem(&self, text: String) {
        // Problems de-duplicate by id, so each distinct text gets its own.
        self.problems.raise(
            ProblemKind::PlatformLimitation,
            &platform_problem_subject(&text),
            self.me(),
            text,
            None,
        );
    }

    /// The platform says a problem it raised with this text is gone. Clearing
    /// one that is not open does nothing, like every other problem.
    pub(crate) fn clear_platform_problem(&self, text: &str) {
        self.problems.clear(
            ProblemKind::PlatformLimitation,
            &platform_problem_subject(text),
            self.me(),
        );
    }
}
