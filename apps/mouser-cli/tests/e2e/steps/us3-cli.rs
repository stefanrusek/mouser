//! J1 steps for Story 3: continuous typing, then sleeping and waking computer B
//! and crossing again (quickstart.md section 3, steps 1 and 2).
//!
//! Computer B is a Parallels VM on the same Mac, so "sleep" is
//! `prlctl suspend <vm>` and "wake" is `prlctl resume <vm>` (`$MOUSER_J1_VM`):
//! a real Windows sleep cannot be woken from the runner, and a suspend has the
//! same observable effect on Mouser (B's network disappears and its processes
//! are frozen, then both come back).

use cucumber::{then, when};

use crate::input;
use crate::us2_cli::read_back;
use crate::world::{Computer, J1World, env};

#[when("the owner types a continuous passage for 60 seconds on computer A")]
async fn types_passage(_world: &mut J1World) {
    crate::us2_cli::focus_note();
    input::type_text(&input::continuous_passage());
}

#[then("the text file on computer B contains the continuous passage exactly")]
async fn passage_arrived(_world: &mut J1World) {
    read_back(&input::continuous_passage());
}

fn prlctl(verb: &str) {
    let vm = env("MOUSER_J1_VM");
    let out = Computer::A.run("prlctl", &[verb, &vm]);
    assert!(
        out.status.success(),
        "prlctl {verb} {vm}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[when("computer B is put to sleep")]
async fn sleeps(_world: &mut J1World) {
    prlctl("suspend");
    Computer::A.wait_for("computer B to go to sleep", |_| {
        (!Computer::B.reachable()).then_some(())
    });
}

#[when("computer B wakes from sleep")]
async fn wakes(_world: &mut J1World) {
    prlctl("resume");
    Computer::A.wait_for("computer B to wake", |_| {
        Computer::B.reachable().then_some(())
    });
}

#[when(expr = "{int} seconds pass")]
async fn wait(_world: &mut J1World, seconds: u64) {
    input::settle(seconds);
}
