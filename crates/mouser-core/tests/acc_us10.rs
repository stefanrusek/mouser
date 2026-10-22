//! Acceptance suite for Story 10: every computer on the desk stays up to
//! date without visiting each one.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us10::UpdateWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us10.feature")
        .await;
}
