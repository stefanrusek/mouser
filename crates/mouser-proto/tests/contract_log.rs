use mouser_proto::ids::ComputerId;
use mouser_proto::log::{LogComputer, LogLevel, LogRecord};

fn validator() -> jsonschema::Validator {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/log-record.schema.json")).unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

fn record(event: &str) -> LogRecord {
    LogRecord {
        ts: "2026-05-01T12:00:00.123Z".into(),
        level: LogLevel::Info,
        computer: LogComputer {
            id: ComputerId([0xab; 16]),
            name: "mac".into(),
        },
        event: event.into(),
        peer: None,
        fields: None,
    }
}

#[test]
fn record_is_one_line_and_validates() {
    let mut r = record("net.path_lost");
    let line = r.to_line().unwrap();
    assert!(!line.contains('\n'));
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(validator().is_valid(&v));
    assert_eq!(v["computer"]["id"], "ab".repeat(16));
    assert!(v.get("peer").is_none() && v.get("fields").is_none());

    r.peer = Some(ComputerId([1; 16]));
    r.fields = Some(
        serde_json::json!({"rtt_ms": 3})
            .as_object()
            .unwrap()
            .clone(),
    );
    for level in [
        LogLevel::Error,
        LogLevel::Warn,
        LogLevel::Debug,
        LogLevel::Trace,
    ] {
        r.level = level;
        let v: serde_json::Value = serde_json::from_str(&r.to_line().unwrap()).unwrap();
        assert!(validator().is_valid(&v));
    }
    assert_eq!(v["level"], "info");
    assert_eq!(v["event"], "net.path_lost");
}

#[test]
fn event_must_match_dotted_lowercase_pattern() {
    for ok in ["a", "net.path_lost", "a1.b2.c_3", "0"] {
        assert!(record(ok).to_line().is_ok(), "{ok}");
    }
    for bad in ["", "Net", "a..b", ".a", "a.", "a-b", "a b", "a.B"] {
        let r = record(bad);
        let err = r.to_line().unwrap_err();
        assert_eq!(err.to_string(), format!("invalid log event name: {bad:?}"));
        // The schema agrees.
        let v = serde_json::to_value(&r).unwrap();
        assert!(!validator().is_valid(&v), "{bad}");
    }
}
