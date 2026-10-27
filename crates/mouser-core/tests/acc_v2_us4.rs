//! Acceptance suite for v2 Story 4: tailoring hotkeys, names and pointer
//! behavior. These are the scenarios core observes through the control API
//! and the computers' input; the Settings window itself is the app lane's.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::v2_us4::TailorWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/v2_us4.feature")
        .await;
}
