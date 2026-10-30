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

/// `--wait-for <pid>` waits for that process to end before anything else
/// starts, so the instance it replaces has let go of the single-instance
/// check (FR-005; probe P3 scenario A).
#[cfg(all(target_os = "windows", not(feature = "gui")))]
#[test]
fn wait_for_holds_the_launch_until_that_process_has_ended() {
    use std::process::{Command, Stdio};
    let mut replaced = Command::new("ping")
        .args(["-n", "3", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mouser-app"))
        .args(["--wait-for", &replaced.id().to_string()])
        .output()
        .unwrap();
    // Ended before the launch went on to its no-window message.
    let ended_first = replaced.try_wait().unwrap().is_some();
    replaced.wait().unwrap();
    assert!(ended_first, "the launch waited for the process it replaces");
    assert_eq!(out.status.code(), Some(2));
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
