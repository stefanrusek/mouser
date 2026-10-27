//! Acceptance suite for v2 Story 5: controlling a Windows computer at
//! administrator applications. These are the scenarios core observes over
//! `VirtualPlatform`; the restart itself and the login screen are the app's
//! and the Windows backend's.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::v2_us5::ElevationWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/v2_us5.feature")
        .await;
}
