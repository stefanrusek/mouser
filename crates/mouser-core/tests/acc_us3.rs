//! Acceptance suite for Story 3: the desk keeps working through sleep,
//! network changes and VPNs.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us3::LinkWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us3.feature")
        .await;
}
