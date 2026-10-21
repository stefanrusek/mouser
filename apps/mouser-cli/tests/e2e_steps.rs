//! Compiles the J1 world and step definitions of this lane so that
//! `cargo check -p mouser-cli --features e2e --tests` proves they build. The
//! journey runner `e2e_j1` and its `.feature` file are authored in Convergence.
#![cfg(feature = "e2e")]

#[path = "e2e/input.rs"]
mod input;
#[path = "e2e/steps/us1-cli.rs"]
mod us1_cli;
#[path = "e2e/steps/us2-cli.rs"]
mod us2_cli;
#[path = "e2e/steps/us3-cli.rs"]
mod us3_cli;
#[path = "e2e/world.rs"]
mod world;
