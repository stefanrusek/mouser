//! Acceptance suite for Story 12: the desk user controls a computer that is
//! at its login or system security screen.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us12::LoginWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us12.feature")
        .await;
}
