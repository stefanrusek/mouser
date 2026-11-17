//! The diagnostics sink's types, shared by every backend and the app: a
//! backend writes lines for the owner's diagnostic log through a sink the
//! app hands it. Types only; each backend closes its own set of events.

use std::sync::Arc;

/// The level of a line, as the catalog gives it per event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Debug,
    Info,
    Warn,
}

/// The value of a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagValue {
    Str(String),
    Bool(bool),
    Int(i64),
}

impl std::fmt::Display for DiagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagValue::Str(s) => f.write_str(s),
            DiagValue::Bool(b) => write!(f, "{b}"),
            DiagValue::Int(i) => write!(f, "{i}"),
        }
    }
}

/// Takes a line's level, its event name and its fields.
pub type Diagnostics = Arc<dyn Fn(DiagLevel, &str, Vec<(&'static str, DiagValue)>) + Send + Sync>;
