//! Contract tests for `mouser logs enable|disable|path|tail` (Story 9 sc. 3,
//! FR-010): through the running Mouser, and straight on the log file when it is
//! stopped. `tail` prints raw JSON Lines that validate against log-record.schema.json.

use crate::support::Lab;
use serde_json::{Value, json};

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../specs/001-mouser-v1/contracts/log-record.schema.json"
    ))
    .unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

fn record(n: u32) -> String {
    json!({
        "ts": format!("2026-01-01T00:00:{n:02}Z"),
        "level": "info",
        "computer": {"id": "0".repeat(32), "name": "Nobody"},
        "event": "peer.connected",
        "fields": {"n": n},
    })
    .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn enable_and_disable_switch_diagnostic_logging_on_the_running_mouser() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let logging = alpha.status().await.logging.unwrap();
    assert!(!logging.enabled);

    let enabled = alpha.mouser(&["logs", "enable"]).await;
    assert_eq!(enabled.code, 0, "{}", enabled.stderr);
    assert_eq!(enabled.stdout, format!("Logging is on: {}\n", logging.path));
    assert!(alpha.status().await.logging.unwrap().enabled);
    let json_on = alpha.mouser(&["--json", "logs", "enable"]).await.json();
    assert_eq!(json_on, json!({"path": logging.path}));

    let disabled = alpha.mouser(&["logs", "disable"]).await;
    assert_eq!(
        (disabled.code, disabled.stdout.as_str()),
        (0, "Logging is off\n")
    );
    assert!(!alpha.status().await.logging.unwrap().enabled);
    let json_off = alpha.mouser(&["--json", "logs", "disable"]).await.json();
    assert_eq!(json_off, json!({"path": logging.path}));
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn path_and_tail_follow_the_running_mousers_log() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let path = alpha.status().await.logging.unwrap().path;

    let printed = alpha.mouser(&["logs", "path"]).await;
    assert_eq!(
        (printed.code, printed.stdout.as_str()),
        (0, format!("{path}\n").as_str())
    );
    assert_eq!(
        alpha.mouser(&["--json", "logs", "path"]).await.json(),
        json!({"path": path})
    );

    // Nothing is logged yet.
    let empty = alpha.mouser(&["logs", "tail"]).await;
    assert_eq!((empty.code, empty.stdout.as_str()), (0, ""));

    alpha.mouser(&["logs", "enable"]).await;
    alpha.mouser(&["desk", "create", "Home office"]).await;
    alpha.mouser(&["logs", "disable"]).await;
    let validator = validator();
    let tail = alpha.mouser(&["logs", "tail"]).await;
    assert_eq!(tail.code, 0, "{}", tail.stderr);
    let lines: Vec<&str> = tail.stdout.lines().collect();
    assert!(lines.len() >= 2, "{}", tail.stdout);
    for line in &lines {
        validator
            .validate(&serde_json::from_str(line).unwrap())
            .unwrap();
    }
    assert!(tail.stdout.contains("logging.enabled"), "{}", tail.stdout);
    assert!(tail.stdout.contains("logging.disabled"), "{}", tail.stdout);
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn path_and_tail_work_with_mouser_stopped() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let log = nobody.user_config_dir.join("logs").join("mouser.log");

    let path = nobody.mouser(&["logs", "path"]).await;
    assert_eq!(
        (path.code, path.stdout.as_str()),
        (0, format!("{}\n", log.display()).as_str())
    );
    assert_eq!(
        nobody.mouser(&["--json", "logs", "path"]).await.json(),
        json!({"path": log.to_str().unwrap()})
    );
    // No log file yet: nothing to print.
    let none = nobody.mouser(&["logs", "tail"]).await;
    assert_eq!((none.code, none.stdout.as_str()), (0, ""));

    std::fs::create_dir_all(log.parent().unwrap()).unwrap();
    let records: Vec<String> = (0..25).map(record).collect();
    std::fs::write(&log, records.join("\n") + "\n").unwrap();
    let validator = validator();

    let default = nobody.mouser(&["logs", "tail"]).await;
    assert_eq!(default.code, 0);
    assert_eq!(default.stdout, records[5..].join("\n") + "\n");
    let two = nobody.mouser(&["logs", "tail", "-n", "2"]).await;
    assert_eq!(two.stdout, records[23..].join("\n") + "\n");
    for line in two.stdout.lines() {
        validator
            .validate(&serde_json::from_str(line).unwrap())
            .unwrap();
    }
    let all = nobody
        .mouser(&["--json", "logs", "tail", "-n", "100"])
        .await;
    assert_eq!(all.stdout, records.join("\n") + "\n");

    for command in ["enable", "disable"] {
        let out = nobody.mouser(&["logs", command]).await;
        assert_eq!(out.code, 2, "{command}");
        assert_eq!(out.stderr.trim(), "Mouser is not running");
    }
    lab.shutdown().await;
}
