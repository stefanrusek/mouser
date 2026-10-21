//! J1 steps for Story 3: continuous typing, then sleeping and waking computer B
//! over SSH and crossing again (quickstart.md section 3, steps 1 and 2).
//!
//! Prerequisite on computer B: the ssh user can run `sudo -n pmset` (passwordless
//! sudo for pmset), so a wake can be scheduled before B goes to sleep; a sleeping
//! Mac cannot be woken over SSH.

use cucumber::{then, when};

use crate::input;
use crate::us2_cli::read_back;
use crate::world::{Computer, J1World};

/// Seconds between scheduling the wake and the wake itself.
const WAKE_IN: u32 = 30;

#[when("the owner types a continuous passage for 60 seconds on computer A")]
async fn types_passage(_world: &mut J1World) {
    input::type_text(&input::continuous_passage());
}

#[then("the text file on computer B contains the continuous passage exactly")]
async fn passage_arrived(_world: &mut J1World) {
    read_back(&input::continuous_passage());
}

#[when("computer B is put to sleep")]
async fn sleeps(_world: &mut J1World) {
    let script = format!(
        "sudo -n pmset schedule wake \"$(date -v+{WAKE_IN}S '+%m/%d/%y %H:%M:%S')\" \
         && (nohup sh -c 'sleep 2; pmset sleepnow' >/dev/null 2>&1 &)"
    );
    let started = Computer::B.run("sh", &["-c", &script]);
    assert!(
        started.status.success(),
        "schedule a wake and sleep B: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    Computer::A.wait_for("computer B to go to sleep", |_| {
        (!Computer::B.run("true", &[]).status.success()).then_some(())
    });
}

#[when("computer B wakes from sleep")]
async fn wakes(_world: &mut J1World) {
    Computer::A.wait_for("computer B to wake", |_| {
        Computer::B.run("true", &[]).status.success().then_some(())
    });
    // Assert user activity so the display is on, as after a keypress.
    let awake = Computer::B.run("caffeinate", &["-u", "-t", "1"]);
    assert!(awake.status.success(), "wake B's display");
}

#[when(expr = "{int} seconds pass")]
async fn wait(_world: &mut J1World, seconds: u64) {
    input::settle(seconds);
}
