//! Acceptance suite for Story 13: the desk owner runs Mouser without installing it.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us13::PortableWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us13.feature")
        .await;
}
