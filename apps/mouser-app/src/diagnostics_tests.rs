use std::sync::Arc;

use mouser_core::logging::{FILE, Log};

use crate::diagnostics::debug_to;

#[test]
fn diagnostics_become_debug_lines_in_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let me = "00".repeat(16).parse().unwrap();
    let log = Arc::new(Log::new(dir.path(), me, "Alpha".into(), true, 1));
    let sink = debug_to(log);
    sink(
        "capture.probe",
        vec![("outcome", "missed".into()), ("hook", "mouse".into())],
    );
    let text = std::fs::read_to_string(dir.path().join("logs").join(FILE)).unwrap();
    let line: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(line["level"], "debug");
    assert_eq!(line["event"], "capture.probe");
    assert_eq!(line["fields"]["outcome"], "missed");
    assert_eq!(line["fields"]["hook"], "mouse");
}
