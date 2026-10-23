//! Debug-level diagnostics: lines for the owner's diagnostic log that help
//! tell causes apart later (which hook missed a probe, what was in front).
//! The platform seam carries only problems, so the app hands this backend
//! a sink that writes to the log.

use std::sync::{Arc, RwLock};

/// Takes an event name (dotted lowercase) and its fields.
pub type Diagnostics = Arc<dyn Fn(&str, Vec<(&'static str, String)>) + Send + Sync>;

static SINK: RwLock<Option<Diagnostics>> = RwLock::new(None);

/// Where this backend's debug-level diagnostics go from now on; `None`
/// drops them (the default).
pub fn set_diagnostics(sink: Option<Diagnostics>) {
    *SINK.write().unwrap() = sink;
}

/// Writes one diagnostic line, if anyone listens.
pub(crate) fn note(event: &str, fields: Vec<(&'static str, String)>) {
    if let Some(sink) = SINK.read().unwrap().as_ref() {
        sink(event, fields);
    }
}
