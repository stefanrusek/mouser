//! Service <-> session-engine IPC (contracts/service-ipc.md). Plain serde
//! types, OS-neutral; framed with the `wire` framing (4-byte big-endian length
//! then postcard).

use serde::{Deserialize, Serialize};

/// The service's control pipe.
pub const SERVICE_CONTROL_PIPE: &str = r"\\.\pipe\mouser-service";

/// The pipe the service creates for the engine in console session `session`.
pub fn engine_pipe_name(session: u32) -> String {
    format!(r"\\.\pipe\mouser-svc-{session}")
}

/// Why the engine is exiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    Quit,
    Fault,
}

/// Outcome of a `SendSas` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SasResult {
    Sent,
    /// `SoftwareSASGeneration` is not 1 or 3.
    PolicyBlocked,
    Failed(String),
}

/// Engine to service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineMessage {
    /// Sent first.
    Hello { version: String },
    /// Ask for Ctrl-Alt-Del.
    SendSas,
    /// Sent before a voluntary exit.
    Stopping { reason: StopReason },
}

/// Service to engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceMessage {
    SasResult(SasResult),
    /// The service is stopping or the console session changed.
    Shutdown,
}
