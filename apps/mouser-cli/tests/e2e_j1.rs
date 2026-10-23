//! Journey J1 on two real computers (a Mac and a Windows 11 VM). Run only
//! through `cargo xtask gate --e2e j1`; see tests/e2e/README.md.
#![cfg(feature = "e2e")]

use cucumber::World;

#[path = "e2e/active.rs"]
mod active;
#[path = "e2e/input.rs"]
mod input;
#[path = "e2e/layout.rs"]
mod layout;
#[path = "e2e/steps/us1-cli.rs"]
mod us1_cli;
#[path = "e2e/steps/us2-cli.rs"]
mod us2_cli;
#[path = "e2e/steps/us3-cli.rs"]
mod us3_cli;
#[path = "e2e/world.rs"]
mod world;

#[tokio::main]
async fn main() {
    world::J1World::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(1)
        .run_and_exit(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/e2e/features/e2e_j1_two_computers.feature"
        ))
        .await;
}
