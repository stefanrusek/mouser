//! Acceptance suite for Story 7: files can be dragged from one computer to another.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us7::DragWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us7.feature")
        .await;
}
