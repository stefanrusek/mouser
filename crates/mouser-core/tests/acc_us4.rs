//! Acceptance suite for Story 4: arrange displays to match the physical desk.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us4::ArrangeWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us4.feature")
        .await;
}
