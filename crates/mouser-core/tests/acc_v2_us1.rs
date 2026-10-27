//! Acceptance suite for v2 Story 1: my pointer and keyboard keep working
//! whatever goes wrong inside Mouser.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::v2_us1::PointerWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/v2_us1.feature")
        .await;
}
