use std::sync::Arc;

use mouser_core::logging::{FILE, Log};
use mouser_platform_windows::{DiagLevel, DiagValue};

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
}
