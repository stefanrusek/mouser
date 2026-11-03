//! Unit tests of `world.rs`, run by the `e2e_steps` test target.

use crate::world::{engine_replaced, parse_pids, runs_as_service, task_action};

#[test]
fn a_desktop_task_runs_powershell_under_a_headless_console() {
    // A plain console program started by Task Scheduler is handed to Windows
    // Terminal (the default terminal), which brings an existing Terminal window
    // to the front and takes the foreground from the J1 note window.
    let action = task_action(r"C:\Users\Public\mouser-j1-note.ps1");
    assert_eq!(
        action,
        r"conhost.exe --headless powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\Users\Public\mouser-j1-note.ps1"
    );
}

#[test]
fn a_computer_whose_own_entry_says_service_runs_as_the_service() {
    let status = serde_json::json!({"computers": [
        {"connection": "connected", "mode": "installed"},
        {"connection": "self", "mode": "service"}
    ]});
    assert!(runs_as_service(&status));
}

#[test]
fn another_computers_service_mode_is_not_this_computers() {
    let status = serde_json::json!({"computers": [
        {"connection": "self", "mode": "installed"},
        {"connection": "connected", "mode": "service"}
    ]});
    assert!(!runs_as_service(&status));
}

#[test]
fn a_status_without_computers_or_mode_is_not_service() {
    assert!(!runs_as_service(&serde_json::json!({})));
    assert!(!runs_as_service(
        &serde_json::json!({"computers": [{"connection": "self"}]})
    ));
}

#[test]
fn process_ids_are_read_one_per_line_and_sorted() {
    assert_eq!(parse_pids("4120\r\n 88 \r\n"), vec![88, 4120]);
    assert_eq!(parse_pids(""), Vec::<u32>::new());
}

#[test]
fn the_engine_is_replaced_only_by_new_process_ids() {
    assert!(engine_replaced(&[10, 11], &[12]));
    assert!(!engine_replaced(&[10, 11], &[]));
    assert!(!engine_replaced(&[10, 11], &[11]));
    assert!(!engine_replaced(&[10, 11], &[12, 10]));
}
