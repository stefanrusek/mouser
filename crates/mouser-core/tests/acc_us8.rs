//! Acceptance suite for Story 8: Mouser tells the desk owner what is wrong
//! and how to fix it.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us8::HealthWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    for lab in &mut world.labs {
                        lab.shutdown().await;
                    }
                }
            })
        })
        .run_and_exit("tests/features/us8.feature")
        .await;
}
