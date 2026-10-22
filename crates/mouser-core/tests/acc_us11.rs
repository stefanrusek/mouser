//! Acceptance suite for Story 11: the desk user jumps the pointer straight
//! to any display with one hotkey and a letter.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us11::JumpWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us11.feature")
        .await;
}
