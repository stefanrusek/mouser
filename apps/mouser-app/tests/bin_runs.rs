/// `--cursor-guard` is read before anything else starts: on Windows the
/// guard for a process that has already ended has nothing to wait for and
/// ends at once, quietly; elsewhere the flag is refused with exit code 3.
#[test]
fn the_cursor_guard_flag_is_handled_before_anything_else() {
    use std::process::Command;
    // Process ids are multiples of four: this one names no process.
    let out = Command::new(env!("CARGO_BIN_EXE_mouser-app"))
        .args(["--cursor-guard", &u32::MAX.to_string()])
        .output()
        .unwrap();
    let want = if cfg!(windows) { 0 } else { 3 };
    assert_eq!(out.status.code(), Some(want));
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

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
