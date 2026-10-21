//! Diagnostic log record (one JSON object per line).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::ComputerId;

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// The logging computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogComputer {
    pub id: ComputerId,
    pub name: String,
}

/// Event name does not match `^[a-z0-9_]+(\.[a-z0-9_]+)*$`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidEvent(pub String);

impl fmt::Display for InvalidEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid log event name: {:?}", self.0)
    }
}

impl std::error::Error for InvalidEvent {}

/// One log line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogRecord {
    /// RFC 3339 UTC.
    pub ts: String,
    pub level: LogLevel,
    pub computer: LogComputer,
    /// Dotted lowercase name.
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<ComputerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Map<String, serde_json::Value>>,
}

fn event_is_valid(event: &str) -> bool {
    event.split('.').all(|seg| {
        !seg.is_empty()
            && seg
                .bytes()
                .all(|c| matches!(c, b'a'..=b'z' | b'0'..=b'9' | b'_'))
    })
}

impl LogRecord {
    /// Serialize to a single JSON line (no trailing newline), checking the event name.
    pub fn to_line(&self) -> Result<String, InvalidEvent> {
        if !event_is_valid(&self.event) {
            return Err(InvalidEvent(self.event.clone()));
        }
        Ok(serde_json::to_string(self).expect("log record serializes"))
    }
}
