//! Acceptance suite for v2 Story 3: one window where the desk owner finds
//! everything. These are the scenarios core observes through `status` and
//! the control API; the window-only ones are the app lane's.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::v2_us3::WindowWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                    world.apart.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/v2_us3.feature")
        .await;
}
