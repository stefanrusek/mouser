use mouser_platform::Os;
use std::process::ExitCode;

use crate::args::{Launch, first, parse};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|a| a.to_string()).collect()
}

#[test]
fn no_flag_is_a_normal_launch() {
    assert_eq!(parse(&args(&["mouser"]), Os::Windows), Ok(Launch::Normal));
    assert_eq!(
        parse(&args(&["mouser", "--headless"]), Os::MacOs),
        Ok(Launch::Normal)
    );
}

#[test]
fn cursor_guard_with_a_pid_selects_guard_mode_on_windows() {
    assert_eq!(
        parse(&args(&["mouser", "--cursor-guard", "4242"]), Os::Windows),
        Ok(Launch::CursorGuard(4242))
    );
}

#[test]
fn cursor_guard_is_rejected_with_exit_code_3_on_other_systems() {
    for os in [Os::MacOs, Os::Linux] {
        assert_eq!(
            parse(&args(&["mouser", "--cursor-guard", "4242"]), os),
            Err(3)
        );
    }
}

#[test]
#[should_panic(expected = "--cursor-guard is followed by a process id")]
fn cursor_guard_without_a_pid_is_a_bug_in_the_caller() {
    let _ = parse(&args(&["mouser", "--cursor-guard"]), Os::Windows);
}

/// A stand-in guard that ends with the pid it was given, so a test can see
/// it ran and for whom.
fn guard_ends_with_pid(pid: u32) -> ExitCode {
    ExitCode::from(pid as u8)
}

#[test]
fn a_normal_launch_carries_on_without_the_guard() {
    assert_eq!(
        first(
            &args(&["mouser", "--headless"]),
            Os::Windows,
            Some(guard_ends_with_pid)
        ),
        None
    );
}

#[test]
fn a_guard_launch_runs_the_guard_for_that_pid_and_ends_with_its_code() {
    assert_eq!(
        first(
            &args(&["mouser", "--cursor-guard", "42"]),
            Os::Windows,
            Some(guard_ends_with_pid)
        ),
        Some(ExitCode::from(42))
    );
}

#[test]
fn a_guard_launch_off_windows_ends_with_exit_code_3_and_runs_no_guard() {
    assert_eq!(
        first(&args(&["mouser", "--cursor-guard", "42"]), Os::Linux, None),
        Some(ExitCode::from(3))
    );
}

#[test]
#[should_panic(expected = "--cursor-guard is followed by a process id")]
fn cursor_guard_with_a_pid_that_is_not_a_number_is_a_bug_in_the_caller() {
    let _ = parse(&args(&["mouser", "--cursor-guard", "abc"]), Os::Windows);
}
