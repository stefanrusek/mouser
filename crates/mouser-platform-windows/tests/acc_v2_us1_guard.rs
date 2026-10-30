//! Acceptance suite for v2 Story 1 scenario 6 on Windows: Mouser ends
//! unexpectedly while another computer has the pointer, and the cursor
//! guard brings the pointer back.
//!
//! This binary plays every part. Run plainly it is the suite. Run with
//! `--helper-parent` it stands in for Mouser: it grabs a real capture, which
//! starts a real guard, this binary again with `--cursor-guard <pid>`.

#[cfg(windows)]
mod steps {
    pub mod v2_us1_guard;
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = steps::v2_us1_guard::role(&args) {
        return code;
    }
    use cucumber::World;
    tokio::runtime::Runtime::new().expect("a runtime").block_on(
        steps::v2_us1_guard::GuardWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit("tests/features/v2_us1_guard.feature"),
    );
    std::process::ExitCode::SUCCESS
}

/// The guard exists on Windows only.
#[cfg(not(windows))]
fn main() {}
