//! Acceptance suite for Story 6: copy and paste work between computers.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us6::ClipboardWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us6.feature")
        .await;
}
