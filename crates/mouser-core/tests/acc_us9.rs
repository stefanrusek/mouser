//! Acceptance suite for Story 9: a troubleshooting agent inspects and
//! changes Mouser from the command line, the configuration file and the
//! diagnostic logs.

mod steps;
mod support;

use cucumber::World;

#[tokio::main]
async fn main() {
    steps::us9::AgentWorld::cucumber()
        .fail_on_skipped()
        .max_concurrent_scenarios(4)
        .after(|_, _, _, _, world| {
            Box::pin(async move {
                if let Some(world) = world {
                    world.lab.shutdown().await;
                }
            })
        })
        .run_and_exit("tests/features/us9.feature")
        .await;
}
