use crate::bus::{address, block_on, connect, connect_system, system_address};
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

#[test]
fn system_bus_address_prefers_the_variable_then_the_standard_socket() {
    assert_eq!(
        system_address(&env(&[("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/s")])),
        "unix:path=/s"
    );
    assert_eq!(
        system_address(&env(&[])),
        "unix:path=/var/run/dbus/system_bus_socket"
    );
}

#[test]
fn connecting_to_a_missing_system_bus_fails() {
    let env = env(&[("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent/bus")]);
    assert!(block_on(async move { connect_system(&env).await.is_err() }));
}
