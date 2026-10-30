//! Local control API: JSON-RPC 2.0 types, `Status`, `Problem` (contracts/control-api.md).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{ComputerId, DeskId};
use crate::model::{DisplayId, Layout, Os, PermissionKind, PermissionState};
use crate::wire::ComputerMode;

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
    ElevatedApp,
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
    RestartElevated(ComputerId),
    KeepPlacement(ComputerId),
    UpdateComputer(ComputerId),
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
            Self::RestartElevated(id) => write!(f, "restart_elevated:{id}"),
            Self::KeepPlacement(id) => write!(f, "keep_placement:{id}"),
            Self::UpdateComputer(id) => write!(f, "update_computer:{id}"),
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
        let computer = |prefix: &str| -> Option<Result<ComputerId, ParseFixError>> {
            s.strip_prefix(prefix)
                .map(|id| id.parse().map_err(|_| err()))
        };
        if let Some(id) = computer("restart_elevated:") {
            return id.map(Self::RestartElevated);
        }
        if let Some(id) = computer("keep_placement:") {
            return id.map(Self::KeepPlacement);
        }
        if let Some(id) = computer("update_computer:") {
            return id.map(Self::UpdateComputer);
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
    /// Program name only (FR-003); set for `elevated_app`. Always serialized
    /// (`null` when absent): `Problem` also travels as postcard.
    #[serde(default)]
    pub app: Option<String>,
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
    /// Absent for 0.2.x peers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    pub version: String,
    /// Absent for 0.2.x peers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ComputerMode>,
    pub connection: ConnectionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<String>,
    pub permissions: BTreeMap<PermissionKind, PermissionState>,
    pub limitations: Vec<String>,
    pub displays: u32,
}

/// Tray connection summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayConnection {
    Connected,
    Alone,
    Disconnected,
}

/// What the tray shows (status v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraySummary {
    pub state: EngineState,
    pub connection: TrayConnection,
    /// At least 1 (this computer).
    pub computers: u32,
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
    /// This computer's id when Local.
    pub active_computer: ComputerId,
    pub tray_summary: TraySummary,
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
    LogsCollect,
    LogsCollectStream,
    DeskRename,
    LayoutKeepPlacement,
    ElevationRestart,
    ProblemFix,
    TransferResolve,
}

impl Method {
    pub const ALL: [Method; 27] = [
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
        Self::LogsCollect,
        Self::LogsCollectStream,
        Self::DeskRename,
        Self::LayoutKeepPlacement,
        Self::ElevationRestart,
        Self::ProblemFix,
        Self::TransferResolve,
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
            Self::LogsCollect => "logs.collect",
            Self::LogsCollectStream => "logs.collect_stream",
            Self::DeskRename => "desk.rename",
            Self::LayoutKeepPlacement => "layout.keep_placement",
            Self::ElevationRestart => "elevation.restart",
            Self::ProblemFix => "problem.fix",
            Self::TransferResolve => "transfer.resolve",
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
    Config,
    Problem,
    JoinRequest,
    Transfer,
    Status,
    EngineEvent,
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

/// Who made a `config.set` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOrigin {
    App,
    #[default]
    Cli,
}

/// `config.set` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSetParams {
    pub key: String,
    pub value: Value,
    /// Defaults to `cli`.
    #[serde(default)]
    pub origin: SetOrigin,
}

/// Where a setting change came from (`config` topic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingOrigin {
    App,
    Cli,
    File,
    DeskMate,
}

/// `config` topic notification: one per changed key (data-model "Setting change").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingChange {
    pub key: String,
    pub old: Value,
    pub new: Value,
    pub origin: SettingOrigin,
    /// The desk mate it came from, for `desk_mate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<ComputerId>,
}

/// Why a computer is not in a log bundle (log-bundle.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleReason {
    Offline,
    LoggingOff,
    OlderVersion,
}

/// One computer of a log bundle, as `logs.collect` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleComputer {
    pub id: ComputerId,
    pub name: String,
    pub included: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<BundleReason>,
}

/// `logs.collect` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsCollectParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

/// `logs.collect` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsCollectResult {
    pub path: String,
    pub computers: Vec<BundleComputer>,
}

/// `logs.collect_stream` result, sent after the last `logs.chunk`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsCollectStreamResult {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub computers: Vec<BundleComputer>,
}

/// `logs.chunk` notification params: `seq` from 0, `data` base64 of at most
/// 65536 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsChunk {
    pub seq: u32,
    pub data: String,
}

impl LogsChunk {
    /// The notification's method name.
    pub const METHOD: &'static str = "logs.chunk";
}

/// `desk.rename` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskRenameParams {
    pub computer: ComputerId,
    pub name: String,
}

/// `layout.keep_placement` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeepPlacementParams {
    pub computer: ComputerId,
}

/// `elevation.restart` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElevationRestartParams {
    pub computer: ComputerId,
}

/// `problem.fix` params: the open problem `(problem_id, computer)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemFixParams {
    pub problem_id: String,
    pub computer: ComputerId,
}

/// `transfer.resolve` params: the answer to a `transfer` notification in
/// state `awaiting_overwrite_decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferResolveParams {
    pub transfer_id: String,
    pub overwrite: bool,
}

/// `engine_event` topic notification: the UI-facing in-process engine
/// events, for an app in UI mode (`{"event": "<name>", ...}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    /// The pointer arrived on this computer: `x`, `y` are logical pixels of
    /// `display`; `desktop` is the same point in desktop coordinates.
    Landing {
        display: DisplayId,
        x: f32,
        y: f32,
        desktop: (f64, f64),
    },
    /// Offer the desk owner a review of a computer's placement.
    ReviewOffered { computer: ComputerId, name: String },
    /// Show each letter on the display it names.
    QuickJumpShow { letters: Vec<(DisplayId, char)> },
    /// Remove the QuickJump letters.
    QuickJumpHide,
    /// Run the elevated restart sequence.
    RestartElevated,
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
