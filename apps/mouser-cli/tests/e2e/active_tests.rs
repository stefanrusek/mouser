//! Unit tests of `active.rs`, run by the `e2e_steps` test target.

use crate::active::is_active;
use serde_json::json;

#[test]
fn another_computer_is_active_only_when_status_names_it() {
    assert!(is_active(&json!({"active_computer": "bb"}), "bb", false));
    assert!(!is_active(&json!({"active_computer": "bb"}), "aa", false));
    assert!(!is_active(&json!({"active_computer": null}), "aa", false));
}

#[test]
fn own_computer_is_active_when_status_names_it_or_says_null() {
    assert!(is_active(&json!({"active_computer": null}), "aa", true));
    assert!(is_active(&json!({"active_computer": "aa"}), "aa", true));
    assert!(!is_active(&json!({"active_computer": "bb"}), "aa", true));
}
