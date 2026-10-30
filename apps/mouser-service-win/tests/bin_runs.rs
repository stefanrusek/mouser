//! The binary itself, without touching the computer: run outside the SCM it
//! says so, and it refuses a command line it does not know.

use std::process::Command;

#[test]
fn outside_the_scm_it_exits_1_and_says_why() {
    let out = Command::new(env!("CARGO_BIN_EXE_mouser-service"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let said = String::from_utf8(out.stderr).unwrap();
    assert!(said.starts_with("running as MouserService: "), "{said}");
}

#[test]
fn an_unknown_command_line_exits_3() {
    let out = Command::new(env!("CARGO_BIN_EXE_mouser-service"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}
