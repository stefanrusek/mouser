//! A platform backend's diagnostics (the Windows capture watchdog's probe
//! outcomes, capture losses, the cursor guard), written to the diagnostic
//! log.

use std::sync::Arc;

use mouser_core::logging::Log;
use mouser_platform_windows::{DiagLevel, DiagValue, Diagnostics};
use mouser_proto::log::LogLevel;
use serde_json::Value;

/// Takes a level, an event name and its fields, as `mouser_platform_windows::Diagnostics`.
pub type Sink = Diagnostics;

/// Writes each diagnostic to `log` at its level (only while logging is on).
pub fn debug_to(log: Arc<Log>) -> Sink {
    Arc::new(move |level, event, fields| {
        let level = match level {
            DiagLevel::Debug => LogLevel::Debug,
            DiagLevel::Info => LogLevel::Info,
            DiagLevel::Warn => LogLevel::Warn,
        };
        let fields = fields
            .into_iter()
            .map(|(key, value)| {
                let value = match value {
                    DiagValue::Str(s) => Value::String(s),
                    DiagValue::Bool(b) => Value::Bool(b),
                    DiagValue::Int(i) => Value::from(i),
                };
                (key.to_string(), value)
            })
            .collect();
        log.record(level, event, None, Some(fields));
    })
}
