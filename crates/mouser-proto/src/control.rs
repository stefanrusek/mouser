//! Local control API: JSON-RPC 2.0 types, `Status`, `Problem` (contracts/control-api.md).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{ComputerId, DeskId};
use crate::model::{Layout, Os, PermissionKind, PermissionState};

/// Problem kinds (data-model.md "Problem"); also `error.data.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    MissingPermission,
    IncompatibleVersion,
    JoinRefused,
    SecureInput,
    CrossingBlocked,
    PeerOffline,
    PlatformLimitation,
}

/// Suggested fix for a problem. Serialized as a string, e.g.
/// `open_system_settings:accessibility`, `approve_join`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixAction {
    OpenSystemSettings(PermissionKind),
    ApproveJoin,
    OpenUpdate,
    CrossAnywayHint,
    OpenArrangerEdge,
}

/// Unrecognised [`FixAction`] string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFixError(pub String);

impl fmt::Display for ParseFixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown fix action: {:?}", self.0)
    }
}

impl std::error::Error for ParseFixError {}

impl fmt::Display for FixAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenSystemSettings(kind) => {
                let name = serde_json::to_value(kind).expect("kind serializes");
                write!(f, "open_system_settings:{}", name.as_str().expect("string"))
            }
            Self::ApproveJoin => f.write_str("approve_join"),
            Self::OpenUpdate => f.write_str("open_update"),
            Self::CrossAnywayHint => f.write_str("cross_anyway_hint"),
            Self::OpenArrangerEdge => f.write_str("open_arranger_edge"),
        }
    }
}

impl FromStr for FixAction {
    type Err = ParseFixError;
    fn from_str(s: &str) -> Result<Self, ParseFixError> {
        let err = || ParseFixError(s.to_string());
        if let Some(kind) = s.strip_prefix("open_system_settings:") {
            let kind = serde_json::from_value(Value::String(kind.into())).map_err(|_| err())?;
            return Ok(Self::OpenSystemSettings(kind));
        }
        match s {
            "approve_join" => Ok(Self::ApproveJoin),
            "open_update" => Ok(Self::OpenUpdate),
            "cross_anyway_hint" => Ok(Self::CrossAnywayHint),
            "open_arranger_edge" => Ok(Self::OpenArrangerEdge),
            _ => Err(err()),
        }
    }
}

impl Serialize for FixAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for FixAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A user-visible problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// Stable kind + subject key.
    pub problem_id: String,
    pub kind: ProblemKind,
    pub computer: ComputerId,
    /// Plain-language text.
    pub message: String,
    pub fix: Option<FixAction>,
    /// RFC 3339 UTC.
    pub raised_at: String,
    pub cleared_at: Option<String>,
}

/// `{id, name}` of a computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerRef {
    pub id: ComputerId,
    pub name: String,
}

/// `{id, name}` of a desk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskRef {
    pub id: DeskId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Active,
    Paused,
}

/// The desk as seen in `status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDesk {
    pub id: DeskId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_join: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    #[serde(rename = "self")]
    SelfComputer,
    Connected,
    Reconnecting,
    Offline,
    Incompatible,
}

/// One computer in `status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusComputer {
    pub id: ComputerId,
    pub name: String,
    pub os: Os,
    pub version: String,
    pub connection: ConnectionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    pub permissions: BTreeMap<PermissionKind, PermissionState>,
    pub limitations: Vec<String>,
    pub displays: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingStatus {
    pub enabled: bool,
    pub path: String,
}

/// Result of `status`; validates against `schemas/status.schema.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub computer: ComputerRef,
    pub state: EngineState,
    pub desk: Option<StatusDesk>,
    pub active_computer: Option<ComputerId>,
    pub computers: Vec<StatusComputer>,
    pub problems: Vec<Problem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingStatus>,
}

// ---- JSON-RPC envelope ------------------------------------------------------

/// Every method of the control API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Status,
    DesksDiscovered,
    DeskCreate,
    DeskJoin,
    DeskLeave,
    DeskMove,
    DeskRemoveMember,
    JoinPending,
    JoinDecide,
    LayoutGet,
    LayoutSet,
    ConfigGet,
    ConfigSet,
    LoggingSet,
    ActivePause,
    ActiveResume,
    SendCad,
    UpdateCheck,
    UpdateApply,
    Subscribe,
}

impl Method {
    pub const ALL: [Method; 20] = [
        Self::Status,
        Self::DesksDiscovered,
        Self::DeskCreate,
        Self::DeskJoin,
        Self::DeskLeave,
        Self::DeskMove,
        Self::DeskRemoveMember,
        Self::JoinPending,
        Self::JoinDecide,
        Self::LayoutGet,
        Self::LayoutSet,
        Self::ConfigGet,
        Self::ConfigSet,
        Self::LoggingSet,
        Self::ActivePause,
        Self::ActiveResume,
        Self::SendCad,
        Self::UpdateCheck,
        Self::UpdateApply,
        Self::Subscribe,
    ];

    /// The JSON-RPC method name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::DesksDiscovered => "desks.discovered",
            Self::DeskCreate => "desk.create",
            Self::DeskJoin => "desk.join",
            Self::DeskLeave => "desk.leave",
            Self::DeskMove => "desk.move",
            Self::DeskRemoveMember => "desk.remove_member",
            Self::JoinPending => "join.pending",
            Self::JoinDecide => "join.decide",
            Self::LayoutGet => "layout.get",
            Self::LayoutSet => "layout.set",
            Self::ConfigGet => "config.get",
            Self::ConfigSet => "config.set",
            Self::LoggingSet => "logging.set",
            Self::ActivePause => "active.pause",
            Self::ActiveResume => "active.resume",
            Self::SendCad => "send_cad",
            Self::UpdateCheck => "update.check",
            Self::UpdateApply => "update.apply",
            Self::Subscribe => "subscribe",
        }
    }

    /// Look a method up by its JSON-RPC name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// Build a request for `method` with already-serialized params.
    pub fn new(id: u64, method: Method, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.name().into(),
            params,
        }
    }
}

/// `error.data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorData {
    pub kind: ProblemKind,
}

/// JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<ErrorData>,
}

/// JSON-RPC 2.0 response: exactly one of `result` / `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// `subscribe` topics, also the notification method names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Problem,
    JoinRequest,
    Transfer,
    Status,
}

/// JSON-RPC notification sent to subscribers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: Topic,
    pub params: Value,
}

// ---- Params / results -------------------------------------------------------

/// Result of methods returning `{}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Empty {}

/// `desks.discovered` item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredDesk {
    pub desk_id: DeskId,
    pub name: String,
    /// Member names.
    pub members: Vec<String>,
}

/// `desk.create` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskCreateParams {
    pub name: String,
}

/// `desk.create` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Desk {
    pub desk_id: DeskId,
    pub name: String,
    pub state: crate::model::DeskState,
}

/// `desk.join` params: `{desk_id}` or `{address}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeskJoinParams {
    ById { desk_id: DeskId },
    ByAddress { address: String },
}

/// `desk.move` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskMoveParams {
    pub desk_id: DeskId,
}

/// `{state: "pending"}` result of `desk.join` / `desk.move`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinPending {
    pub state: PendingState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingState {
    Pending,
}

/// `desk.remove_member` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveMemberParams {
    pub computer_id: ComputerId,
}

/// `join.pending` item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingJoin {
    pub request_id: u64,
    pub computer: ComputerRef,
    pub desk: DeskRef,
}

/// `join.decide` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinDecideParams {
    pub request_id: u64,
    pub approve: bool,
}

/// `layout.set` params and `layout.get` / `layout.set` result.
pub type LayoutResult = Layout;

/// `config.get` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigGetParams {
    pub key: Option<String>,
}

/// `config.set` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSetParams {
    pub key: String,
    pub value: Value,
}

/// `logging.set` params.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingSetParams {
    pub enabled: bool,
}

/// `logging.set` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingSetResult {
    pub path: String,
}

/// `update.check` / `update.apply` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateResult {
    pub current: String,
    pub available: Option<String>,
}

/// `subscribe` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeParams {
    pub topics: Vec<Topic>,
}
