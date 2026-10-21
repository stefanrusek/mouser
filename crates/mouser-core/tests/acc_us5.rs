//! Acceptance suite for Story 5: the pointer stays put when the desk user does not mean to cross.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us5::BlockWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us5.feature")
        .await;
}
