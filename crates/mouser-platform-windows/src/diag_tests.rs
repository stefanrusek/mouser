use crate::diag::{note, set_diagnostics};
use std::sync::{Arc, Mutex};

#[test]
fn diagnostics_reach_the_sink_while_one_is_set() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    note("test.before", vec![]);
    set_diagnostics(Some(Arc::new(move |event, fields| {
        // Other tests' watchdogs may write meanwhile.
        if event.starts_with("test.") {
            sink.lock().unwrap().push((event.to_string(), fields));
        }
    })));
    note("test.during", vec![("hook", "mouse".into())]);
    set_diagnostics(None);
    note("test.after", vec![]);
    assert_eq!(
        *heard.lock().unwrap(),
        [(
            "test.during".to_string(),
            vec![("hook", "mouse".to_string())]
        )]
    );
}
