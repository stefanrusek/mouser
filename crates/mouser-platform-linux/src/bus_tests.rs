use crate::bus::{address, block_on, connect};
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn bus_address_prefers_the_session_variable_then_the_runtime_dir() {
    assert_eq!(
        address(&env(&[
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/a"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ])),
        Some("unix:path=/a".to_string())
    );
    assert_eq!(
        address(&env(&[("XDG_RUNTIME_DIR", "/run/user/1000")])),
        Some("unix:path=/run/user/1000/bus".to_string())
    );
    assert_eq!(address(&env(&[])), None);
}

#[test]
fn connecting_without_a_bus_fails() {
    let err = block_on(async { connect(&env(&[])).await.unwrap_err() });
    assert!(err.to_string().contains("no session bus"), "{err}");
}

#[test]
fn block_on_works_inside_another_runtime() {
    let value = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async { block_on(async { 41 + 1 }) });
    assert_eq!(value, 42);
}
