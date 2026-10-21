//! Acceptance suite for Story 2: one mouse and keyboard drive every computer.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us2::CrossWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us2.feature")
        .await;
}
