//! Acceptance suite for v2 Story 6: Mouser without its window or tray on
//! Windows and Linux. Runs `crates/mouser-core/tests/features/v2_us6.feature`
//! against the headless host over `VirtualPlatform`, alongside a second
//! in-process engine.

#[path = "steps/v2_us6.rs"]
mod v2_us6;

use cucumber::World;

#[tokio::main]
async fn main() {
    v2_us6::HeadlessWorld::cucumber()
        .fail_on_skipped()
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.stop().await;
                }
            })
        })
        .run_and_exit("../../crates/mouser-core/tests/features/v2_us6.feature")
        .await;
}
