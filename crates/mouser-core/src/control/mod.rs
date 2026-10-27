//! The local control API: JSON-RPC 2.0 over a per-OS socket or pipe, and the
//! same methods in-process for the app's UI.

pub mod client;
pub mod config_api;
pub mod layout;
pub mod logs;
pub mod logs_stream;
pub mod server;
pub mod status;
pub mod update;

#[cfg(test)]
mod config_api_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod logs_stream_tests;
#[cfg(test)]
mod logs_tests;
#[cfg(test)]
mod server_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod update_tests;

use std::sync::Arc;

use mouser_proto::control::{Method, Notification, RpcError};
use serde_json::Value;
use tokio::sync::broadcast;

use crate::engine::Ctx;

/// A JSON-RPC "invalid params" error.
pub(crate) fn invalid(message: impl Into<String>) -> RpcError {
    RpcError {
        code: -32602,
        message: message.into(),
        data: None,
    }
}

/// A JSON-RPC "internal error": the operating system refused something.
pub(crate) fn internal(message: impl Into<String>) -> RpcError {
    RpcError {
        code: -32603,
        message: message.into(),
        data: None,
    }
}

/// In-process access to the control API.
#[derive(Clone)]
pub struct ControlHandle {
    ctx: Arc<Ctx>,
}

impl ControlHandle {
    pub(crate) fn new(ctx: Arc<Ctx>) -> Self {
        Self { ctx }
    }

    /// Call `method` as a socket client would.
    pub async fn call(&self, method: Method, params: Option<Value>) -> Result<Value, RpcError> {
        server::call(&self.ctx, method.name(), params).await
    }

    /// The events with no control-API topic (see [`crate::EngineEvent`]).
    pub fn events(&self) -> broadcast::Receiver<crate::EngineEvent> {
        self.ctx.events.subscribe()
    }

    /// Answer the question a `transfer` notification in state
    /// `awaiting_overwrite_decision` asked: replace the file (or not).
    pub fn resolve_transfer(&self, transfer_id: &str, overwrite: bool) {
        // No transfer task means this platform cannot drag files, and then
        // there is no question to answer.
        let _ = self
            .ctx
            .xfer
            .send(crate::transfer::engine::Command::Resolve {
                transfer_id: transfer_id.into(),
                overwrite,
            });
    }

    /// Carry out the fix of the open problem `problem_id` (a problem's
    /// `fix` is what the user is offered): opens the system screen for a
    /// missing permission, and acknowledges `open_arranger_edge`, which
    /// clears that notice. The other fixes are the app's own.
    pub fn fix(&self, problem_id: &str) -> Result<(), String> {
        self.ctx.apply_fix(problem_id)?;
        self.ctx.notify_status();
        Ok(())
    }

    /// Every notification from now on (the caller filters by topic).
    pub fn notifications(&self) -> broadcast::Receiver<Notification> {
        self.ctx.notify.subscribe()
    }
}

/// The per-OS control socket (Unix) or pipe (Windows) path for this user.
pub fn default_path() -> std::path::PathBuf {
    server::default_path_from(|key| std::env::var(key).ok())
}
