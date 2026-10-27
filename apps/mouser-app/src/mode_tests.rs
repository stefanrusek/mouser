use mouser_proto::wire::ComputerMode;

use crate::mode::select;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

#[test]
fn portable_beside_the_executable_is_portable() {
    assert_eq!(
        select(&args(&["mouser"]), true, false),
        ComputerMode::Portable
    );
}

#[test]
fn portable_with_an_elevated_token_is_portable_elevated() {
    assert_eq!(
        select(&args(&["mouser"]), true, true),
        ComputerMode::PortableElevated
    );
}

#[test]
fn headless_is_headless() {
    assert_eq!(
        select(&args(&["mouser", "--headless"]), false, false),
        ComputerMode::Headless
    );
}

#[test]
fn headless_machine_is_the_service() {
    assert_eq!(
        select(&args(&["mouser", "--headless", "--machine"]), false, false),
        ComputerMode::Service
    );
}

#[test]
fn otherwise_the_app_is_installed() {
    assert_eq!(
        select(&args(&["mouser"]), false, false),
        ComputerMode::Installed
    );
    // Elevation alone changes nothing, and --machine means nothing without --headless.
    assert_eq!(
        select(&args(&["mouser", "--machine"]), false, true),
        ComputerMode::Installed
    );
}
