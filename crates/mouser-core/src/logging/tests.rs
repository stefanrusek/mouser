use mouser_proto::control::Method;
use mouser_proto::ids::ComputerId;
use mouser_proto::log::LogLevel;
use serde_json::{Value, json};

use super::*;
use crate::testing::*;

const ME: ComputerId = ComputerId([7; 16]);

fn schema() -> jsonschema::Validator {
    let text = include_str!("../../../../specs/001-mouser-v1/contracts/log-record.schema.json");
    jsonschema::validator_for(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

fn lines(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON record per line"))
        .collect()
}

#[test]
fn nothing_is_written_while_logging_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), false, 1);
    log.record(LogLevel::Info, "peer.connected", None, None);
    assert!(!log.enabled());
    assert!(!log.path().exists());
    assert_eq!(log.path(), dir.path().join("logs").join("mouser.log"));
}

#[test]
fn records_are_json_lines_that_validate_against_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), true, 1);
    let peer = ComputerId([9; 16]);
    let fields = json!({"why": "test"}).as_object().cloned();
    log.record(LogLevel::Warn, "peer.disconnected", Some(peer), fields);
    log.record(LogLevel::Debug, "problem.raised", None, None);
    let written = lines(&log.path());
    assert_eq!(written.len(), 2);
    let schema = schema();
    for record in &written {
        assert!(schema.is_valid(record), "{record}");
    }
    assert_eq!(written[0]["level"], "warn");
    assert_eq!(written[0]["computer"]["name"], "Alpha");
    assert_eq!(written[0]["peer"], peer.to_string());
    assert_eq!(written[0]["fields"]["why"], "test");
    assert_eq!(written[1]["event"], "problem.raised");
}

#[test]
fn turning_logging_on_and_off_is_recorded_and_the_name_follows_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), false, 1);
    log.apply(true, 1, "Desk Mac");
    log.apply(true, 1, "Desk Mac");
    log.record(LogLevel::Info, "peer.connected", None, None);
    log.apply(false, 1, "Desk Mac");
    log.record(LogLevel::Info, "peer.connected", None, None);
    let written = lines(&log.path());
    let events: Vec<_> = written
        .iter()
        .map(|r| r["event"].as_str().unwrap())
        .collect();
    assert_eq!(
        events,
        ["logging.enabled", "peer.connected", "logging.disabled"]
    );
    assert_eq!(written[0]["computer"]["name"], "Desk Mac");
}

#[test]
fn a_full_file_rotates_and_the_old_one_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), true, 1);
    log.set_max_bytes(400);
    for _ in 0..12 {
        log.record(LogLevel::Info, "peer.connected", None, None);
    }
    let rotated = log.path().with_extension("log.1");
    assert!(rotated.exists());
    let size = |p: &std::path::Path| std::fs::metadata(p).unwrap().len();
    assert!(size(&log.path()) <= 400, "{}", size(&log.path()));
    assert!(size(&rotated) <= 400);
    // Every line in both files is whole.
    assert!(!lines(&log.path()).is_empty());
    assert!(!lines(&rotated).is_empty());
}

#[test]
fn the_log_module_never_writes_to_the_console() {
    let source = include_str!("mod.rs");
    for banned in ["println!", "eprintln!", "print!(", "dbg!", "syslog"] {
        assert!(!source.contains(banned), "{banned}");
    }
}

#[tokio::test]
async fn logging_set_returns_the_path_and_starts_writing() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    assert!(!status(&node).await.logging.unwrap().enabled);
    let on: LoggingSetResult = call(&node, Method::LoggingSet, json!({"enabled": true})).await;
    let path = std::path::PathBuf::from(&on.path);
    assert_eq!(path, node.dir.path().join("logs").join("mouser.log"));
    let s = status(&node).await.logging.unwrap();
    assert!(s.enabled);
    assert_eq!(s.path, on.path);
    // The setting is the config's: it survives a restart.
    let config: Value = call(
        &node,
        Method::ConfigGet,
        json!({"key": "diagnostics.logging"}),
    )
    .await;
    assert_eq!(config, json!(true));
    let written = lines(&path);
    assert_eq!(written[0]["event"], "logging.enabled");
    let schema = schema();
    assert!(written.iter().all(|r| schema.is_valid(r)));
    let off: LoggingSetResult = call(&node, Method::LoggingSet, json!({"enabled": false})).await;
    assert_eq!(off.path, on.path);
    assert!(!status(&node).await.logging.unwrap().enabled);
    let last = lines(&path).pop().unwrap();
    assert_eq!(last["event"], "logging.disabled");
}

#[tokio::test]
async fn problems_and_connections_are_logged_with_the_peer() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let b = nodes.pop().unwrap();
    let a = &nodes[0];
    let beta = b.engine.computer_id().to_string();
    let _: LoggingSetResult = call(a, Method::LoggingSet, json!({"enabled": true})).await;
    let path = a.dir.path().join("logs").join("mouser.log");
    b.platform
        .set_secure_input(Some(mouser_platform::SecureInputHolder {
            pid: 1,
            app_name: "Terminal".into(),
        }));
    eventually("the secure input problem to be logged", || async {
        lines(&path).iter().any(|r| r["event"] == "problem.raised")
    })
    .await;
    let raised = lines(&path)
        .into_iter()
        .find(|r| r["event"] == "problem.raised")
        .unwrap();
    assert_eq!(raised["fields"]["kind"], "secure_input");
    assert_eq!(raised["computer"]["name"], "Alpha");
    b.engine.shutdown().await;
    eventually("the disconnect to be logged", || async {
        lines(&path)
            .iter()
            .any(|r| r["event"] == "peer.disconnected" && r["peer"] == beta)
    })
    .await;
}

#[tokio::test]
async fn a_computer_joining_the_desk_is_logged() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let b = lab.start("Beta").await;
    let _: LoggingSetResult = call(&a, Method::LoggingSet, json!({"enabled": true})).await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    admit(&a, &b).await;
    let path = a.dir.path().join("logs").join("mouser.log");
    eventually("the connection to be logged", || async {
        lines(&path).iter().any(|r| {
            r["event"] == "peer.connected" && r["peer"] == b.engine.computer_id().to_string()
        })
    })
    .await;
}

#[test]
fn a_log_that_cannot_be_opened_drops_every_record() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the log directory belongs.
    std::fs::write(dir.path().join("logs"), b"in the way").unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), true, 1);
    log.record(LogLevel::Info, "peer.connected", None, None);
    assert!(log.enabled());
    // Turning logging on later is no different.
    let off = Log::new(dir.path(), ME, "Alpha".into(), false, 1);
    off.apply(true, 1, "Alpha");
    off.record(LogLevel::Info, "peer.connected", None, None);
    assert!(off.enabled());
    assert!(!log.path().exists());
}

#[test]
fn a_record_that_cannot_rotate_the_file_is_dropped_and_logging_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), true, 1);
    log.record(LogLevel::Info, "peer.connected", None, None);
    log.set_max_bytes(10);
    // The previous file cannot be replaced: a directory is in its place.
    let rotated = dir.path().join("logs").join("mouser.log.1");
    std::fs::create_dir(&rotated).unwrap();
    std::fs::write(rotated.join("keep"), b"x").unwrap();
    log.record(LogLevel::Info, "peer.disconnected", None, None);
    assert_eq!(lines(&log.path()).len(), 1, "the record was dropped");
    // Once it can rotate, the next record is written.
    std::fs::remove_dir_all(&rotated).unwrap();
    log.record(LogLevel::Info, "problem.raised", None, None);
    assert!(rotated.is_file());
    let written = lines(&log.path());
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["event"], "problem.raised");
}
