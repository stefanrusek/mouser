use std::process::Command;

#[test]
fn binary_exits_zero() {
    let status = Command::new(env!("CARGO_BIN_EXE_mouser-app"))
        .status()
        .unwrap();
    assert!(status.success());
}
