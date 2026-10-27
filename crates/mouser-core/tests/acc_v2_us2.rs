//! Acceptance suite for v2 Story 2: the diagnostic log shows what Mouser did
//! and why.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::v2_us2::LogWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/v2_us2.feature")
        .await;
}
