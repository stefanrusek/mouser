//! The local control API: JSON-RPC 2.0 over a per-OS socket or pipe, and the
//! same methods in-process for the app's UI.

pub mod client;
pub mod config_api;
pub mod server;
pub mod status;

#[cfg(test)]
mod config_api_tests;
#[cfg(test)]
mod server_tests;
#[cfg(test)]
mod status_tests;

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

    /// Every notification from now on (the caller filters by topic).
    pub fn notifications(&self) -> broadcast::Receiver<Notification> {
        self.ctx.notify.subscribe()
    }
}

/// The per-OS control socket (Unix) or pipe (Windows) path for this user.
pub fn default_path() -> std::path::PathBuf {
    server::default_path_from(|key| std::env::var(key).ok())
}
