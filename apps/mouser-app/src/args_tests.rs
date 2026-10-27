use mouser_platform::Os;

use crate::args::{Launch, parse};

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

#[test]
#[should_panic(expected = "--cursor-guard is followed by a process id")]
fn cursor_guard_with_a_pid_that_is_not_a_number_is_a_bug_in_the_caller() {
    let _ = parse(&args(&["mouser", "--cursor-guard", "abc"]), Os::Windows);
}
