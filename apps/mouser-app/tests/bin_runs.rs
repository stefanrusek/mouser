#[cfg(not(feature = "gui"))]
#[test]
fn binary_without_the_gui_feature_says_so_and_fails() {
    use std::process::Command;
    let out = Command::new(env!("CARGO_BIN_EXE_mouser-app"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("--features gui")
    );
}
