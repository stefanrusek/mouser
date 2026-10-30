//! Acceptance suite for v2 Story 3 scenario 16 on Windows: software that
//! swallows Mouser's watchdog probes is no problem for Health while the
//! desk user's own input keeps arriving.

#[cfg(windows)]
mod steps {
    pub mod v2_us3_watchdog;
}

#[cfg(windows)]
fn main() {
    use cucumber::World;
    tokio::runtime::Runtime::new().expect("a runtime").block_on(
        steps::v2_us3_watchdog::WatchdogWorld::cucumber()
            .fail_on_skipped()
            .run_and_exit("tests/features/v2_us3_watchdog.feature"),
    );
}

/// The watchdog exists on Windows only.
#[cfg(not(windows))]
fn main() {}
