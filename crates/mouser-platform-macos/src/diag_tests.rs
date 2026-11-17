use crate::diag::{emit, heard_during, set_diagnostics};
use mouser_platform::{DiagLevel, DiagValue};
use std::sync::{Arc, Mutex};

fn line() -> Vec<(&'static str, DiagValue)> {
    vec![("aspect", DiagValue::Str("visible".into()))]
}

#[test]
fn a_set_sink_receives_the_line_with_its_level_name_and_fields() {
    let heard = heard_during(|| emit(DiagLevel::Warn, "cursor.foreign_change", line()));
    assert_eq!(
        heard,
        [(DiagLevel::Warn, "cursor.foreign_change".to_string(), line())]
    );
}

#[test]
fn an_unset_sink_drops_lines_silently_and_a_removed_one_stops_hearing() {
    // Takes the listening turn so no other test's sink is set meanwhile.
    let heard = heard_during(|| {});
    assert!(heard.is_empty());
    let seen = Arc::new(Mutex::new(0));
    let into = seen.clone();
    let _ = heard_during(|| {
        set_diagnostics(None);
        emit(DiagLevel::Info, "cursor.foreign_change", line());
        set_diagnostics(Some(Arc::new(move |_, _, _| *into.lock().unwrap() += 1)));
        emit(DiagLevel::Info, "cursor.foreign_change", line());
        set_diagnostics(None);
        emit(DiagLevel::Info, "cursor.foreign_change", line());
    });
    assert_eq!(*seen.lock().unwrap(), 1);
}
