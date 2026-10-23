//! A platform backend's debug-level diagnostics (the Windows capture
//! watchdog's probe outcomes), written to the diagnostic log.

use std::sync::Arc;

use mouser_core::logging::Log;
use mouser_proto::log::LogLevel;
use serde_json::Value;

/// Takes an event name and its fields, as `mouser_platform_windows::Diagnostics`.
pub type Sink = Arc<dyn Fn(&str, Vec<(&'static str, String)>) + Send + Sync>;

/// Writes each diagnostic to `log` at debug level (only while logging is on).
pub fn debug_to(log: Arc<Log>) -> Sink {
    Arc::new(move |event, fields| {
        let fields = fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), Value::String(value)))
            .collect();
        log.record(LogLevel::Debug, event, None, Some(fields));
    })
}
