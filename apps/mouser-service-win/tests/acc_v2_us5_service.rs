//! Acceptance suite for v2 Story 5 (installed Windows) and FR-013:
//! `tests/features/v2_us5_service.feature` against `MouserService`'s state
//! machine, install and uninstall over the fake `WinApi` (OD-5).

#[path = "steps/v2_us5_service.rs"]
mod v2_us5_service;

use cucumber::World;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    v2_us5_service::ServiceWorld::cucumber()
        .fail_on_skipped()
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.stop();
                }
            })
        })
        .run_and_exit("tests/features/v2_us5_service.feature")
        .await;
}
