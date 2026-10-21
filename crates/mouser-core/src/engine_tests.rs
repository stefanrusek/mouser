use std::sync::Arc;

use mouser_proto::control::{Empty, Method};
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use crate::testing::{Lab, call, call_err, status};
use crate::{Engine, EngineParams};

#[tokio::test]
async fn start_serves_the_control_api_in_process_and_shutdown_removes_the_socket() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let path = node.control_path.clone();
    if cfg!(unix) {
        assert!(path.exists(), "the socket is a file on Unix");
    }
    assert_eq!(status(&node).await.computer.name, "Alpha");
    let handle = node.engine.control();
    let mut notes = handle.notifications();
    let _: Empty = call(&node, Method::ActivePause, json!({})).await;
    assert_eq!(
        notes.recv().await.unwrap().method,
        mouser_proto::control::Topic::Status
    );
    let err = call_err(&node, Method::UpdateCheck, json!({})).await;
    assert_eq!(err.code, -32601);
    node.engine.shutdown().await;
    assert!(!path.exists());
}

#[tokio::test]
async fn a_configured_computer_name_replaces_the_operating_system_name() {
    let lab = Lab::new();
    let node = lab
        .start_with("os-name", VirtualPlatform::new(), |c| {
            c.general.computer_name = "Studio".into()
        })
        .await;
    assert_eq!(status(&node).await.computer.name, "Studio");
}

#[tokio::test]
async fn without_a_discovery_backend_the_engine_uses_real_mdns() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(crate::testing::free_port());
    crate::config::save(dir.path(), &config);
    let params = EngineParams::new(
        Arc::new(VirtualPlatform::new()),
        dir.path().to_path_buf(),
        dir.path().join("mouser.sock"),
        "Alpha".into(),
        "0.1.0".into(),
    );
    let engine = Engine::start(params).await;
    assert_eq!(
        engine.local_addr().port(),
        u16::try_from(config.advanced.port).unwrap()
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn events_without_a_control_topic_are_available_in_process() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut events = node.engine.control().events();
    assert_eq!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    );
    assert!(node.engine.layout().placements.is_empty());
    assert!(node.engine.needs_review().is_empty());
}
