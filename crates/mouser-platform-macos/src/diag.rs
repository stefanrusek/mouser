//! Where this backend's diagnostics go: a sink the app hands it, which
//! writes to the diagnostic log. The sink types are shared
//! (`mouser_platform::Diagnostics`); this is the macOS holder, as the
//! Windows backend has its own. Without a sink, lines are dropped.

use mouser_platform::{DiagLevel, DiagValue, Diagnostics};
use std::sync::RwLock;

static SINK: RwLock<Option<Diagnostics>> = RwLock::new(None);

/// Where this backend's diagnostics go from now on; `None` drops them (the
/// default).
pub fn set_diagnostics(sink: Option<Diagnostics>) {
    *SINK.write().unwrap() = sink;
}

/// Writes one line, if anyone listens. The cursor guard is its first caller
/// (the `cursor.foreign_change` event).
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "first production caller is the cursor guard (T015)"
    )
)]
pub(crate) fn emit(level: DiagLevel, name: &str, fields: Vec<(&'static str, DiagValue)>) {
    if let Some(sink) = SINK.read().unwrap().as_ref() {
        sink(level, name, fields);
    }
}

/// A line as the sink receives it.
#[cfg(test)]
pub(crate) type Line = (DiagLevel, String, Vec<(&'static str, DiagValue)>);

/// For tests: every line written while `f` runs. Tests that listen take
/// turns, so one does not take the sink from another.
#[cfg(test)]
pub(crate) fn heard_during(f: impl FnOnce()) -> Vec<Line> {
    use std::sync::Arc;
    static LISTENING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = LISTENING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
    let into = heard.clone();
    set_diagnostics(Some(Arc::new(move |level, event, fields| {
        into.lock()
            .unwrap()
            .push((level, event.to_string(), fields));
    })));
    f();
    set_diagnostics(None);
    std::mem::take(&mut *heard.lock().unwrap())
}
