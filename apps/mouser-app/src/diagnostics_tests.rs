use std::sync::Arc;

use mouser_core::logging::{FILE, Log};
use mouser_platform::{DiagLevel, DiagValue};

use crate::diagnostics::debug_to;

/// The last line of the log in `dir`.
fn last_line(dir: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join("logs").join(FILE)).unwrap();
    serde_json::from_str(text.lines().last().unwrap()).unwrap()
}

#[test]
fn diagnostics_become_lines_in_the_log_at_their_level_with_typed_fields() {
    let dir = tempfile::tempdir().unwrap();
    let me = "00".repeat(16).parse().unwrap();
    let log = Arc::new(Log::new(dir.path(), me, "Alpha".into(), true, 1));
    let sink = debug_to(log);
    sink(
        DiagLevel::Debug,
        "capture.probe",
        vec![
            ("outcome", DiagValue::Str("missed".into())),
            ("hook", DiagValue::Str("mouse".into())),
            ("misses", DiagValue::Int(2)),
        ],
    );
    let line = last_line(dir.path());
    assert_eq!(line["level"], "debug");
    assert_eq!(line["event"], "capture.probe");
    assert_eq!(line["fields"]["outcome"], "missed");
    assert_eq!(line["fields"]["hook"], "mouse");
    assert_eq!(line["fields"]["misses"], 2);
    sink(
        DiagLevel::Info,
        "capture.desktop",
        vec![("desktop", DiagValue::Str("winlogon".into()))],
    );
    assert_eq!(last_line(dir.path())["level"], "info");
    sink(
        DiagLevel::Warn,
        "capture.reinstall",
        vec![("ok", DiagValue::Bool(true))],
    );
    let line = last_line(dir.path());
    assert_eq!(line["level"], "warn");
    assert_eq!(line["fields"]["ok"], true);
    // The macOS cursor guard's event, with its typed fields.
    sink(
        DiagLevel::Warn,
        "cursor.foreign_change",
        vec![
            ("aspect", DiagValue::Str("visible".into())),
            ("phase", DiagValue::Str("remote".into())),
            ("corrected", DiagValue::Bool(true)),
            ("count", DiagValue::Int(3)),
        ],
    );
    let line = last_line(dir.path());
    assert_eq!(line["level"], "warn");
    assert_eq!(line["event"], "cursor.foreign_change");
    assert_eq!(line["fields"]["aspect"], "visible");
    assert_eq!(line["fields"]["corrected"], true);
    assert_eq!(line["fields"]["count"], 3);
}

/// The catalog of log events, with the record schema it refers to.
fn events_validator() -> jsonschema::Validator {
    let record: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/mouser-proto/schemas/log-record.schema.json"
    ))
    .unwrap();
    let events: serde_json::Value = serde_json::from_str(include_str!(
        "../../../crates/mouser-proto/schemas/log-events.schema.json"
    ))
    .unwrap();
    let registry = jsonschema::Registry::new()
        .add("https://mouser.local/schemas/log-record.v1.json", record)
        .unwrap()
        .prepare()
        .unwrap();
    jsonschema::options()
        .with_registry(&registry)
        .build(&events)
        .unwrap()
}

#[test]
fn a_cursor_foreign_change_reaches_the_log_with_the_closed_field_set_and_passes_the_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let me = "00".repeat(16).parse().unwrap();
    let log = Arc::new(Log::new(dir.path(), me, "Alpha".into(), true, 1));
    let sink = debug_to(log);
    sink(
        DiagLevel::Warn,
        "cursor.foreign_change",
        vec![
            ("aspect", DiagValue::Str("position".into())),
            ("phase", DiagValue::Str("arrived".into())),
            ("corrected", DiagValue::Bool(false)),
            ("count", DiagValue::Int(50)),
        ],
    );
    let line = last_line(dir.path());
    let mut fields: Vec<&str> = line["fields"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["aspect", "corrected", "count", "phase"]);
    let validator = events_validator();
    assert!(validator.is_valid(&line), "{line}");
}
