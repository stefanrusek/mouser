//! The closed catalog of diagnostic events (contracts/log-events.md): one
//! constructor per row. Core writes records only through these, so a
//! record's fields are exactly the catalog's and free text (typed keys,
//! clipboard content, file names) cannot reach the log (FR-003).

use std::path::Path;

use mouser_platform::ForegroundApp;
use mouser_proto::control::{BundleReason, Problem, SettingChange, SettingOrigin};
use mouser_proto::ids::ComputerId;
use mouser_proto::log::{LogComputer, LogLevel, LogRecord};
use mouser_proto::model::Os;
use mouser_proto::service_ipc::SasResult;
use mouser_proto::wire::ClipboardKind;
use serde_json::{Map, Value, json};

pub use crate::session::close_reason::LinkLossReason;

/// A closed set of strings the catalog names, with its members in catalog
/// order (`ALL`, for the tests that check every one against the schema).
macro_rules! tagged {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[cfg(test)]
            pub(crate) const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The value the catalog names.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }
        }
    };
}

pub(crate) use tagged;

tagged! {
    /// How control moved (`control.changed`).
    ControlHow {
        Crossing => "crossing",
        Panic => "panic",
        Quickjump => "quickjump",
        Pause => "pause",
        LinkLost => "link_lost",
        CaptureLost => "capture_lost",
        ElevatedApp => "elevated_app",
        Fault => "fault",
        Quit => "quit",
        Entered => "entered",
    }
}

tagged! {
    /// Why the pointer was hidden.
    PointerHidden {
        CrossingOut => "crossing_out",
        Quickjump => "quickjump",
    }
}

tagged! {
    /// Why the pointer was shown.
    PointerShown {
        Returned => "returned",
        CaptureLost => "capture_lost",
        Fault => "fault",
        Quit => "quit",
        StartupRecovery => "startup_recovery",
        Guard => "guard",
    }
}

tagged! {
    /// The kind of input or request the operating system refused.
    InputKind {
        Key => "key",
        Button => "button",
        Motion => "motion",
        Scroll => "scroll",
        Grab => "grab",
        Warp => "warp",
    }
}

tagged! {
    /// What recovered from a fault.
    RecoveryAction {
        ReturnedLocal => "returned_local",
        ClosedLink => "closed_link",
        Continued => "continued",
    }
}

tagged! {
    /// What changed about a display.
    DisplayChange {
        Added => "added",
        Removed => "removed",
        Rearranged => "rearranged",
        Resized => "resized",
    }
}

tagged! {
    /// What changed in the layout.
    LayoutChange {
        Moved => "moved",
        DisplayRemoved => "display_removed",
        DisplayIncluded => "display_included",
        KeptPlacement => "kept_placement",
        AutoPlaced => "auto_placed",
    }
}

tagged! {
    /// How a restart as administrator went.
    ElevationOutcome {
        Prompted => "prompted",
        Declined => "declined",
        HandedOver => "handed_over",
    }
}

/// One record, built only by the constructors below.
#[derive(Debug, Clone)]
pub struct Event {
    level: LogLevel,
    name: String,
    peer: Option<ComputerId>,
    fields: Option<Map<String, Value>>,
}

impl Event {
    fn new(
        level: LogLevel,
        name: &str,
        peer: Option<ComputerId>,
        fields: Option<Map<String, Value>>,
    ) -> Self {
        Self {
            level,
            name: name.to_string(),
            peer,
            fields,
        }
    }

    pub(super) fn into_record(self, computer: LogComputer, ts: String) -> LogRecord {
        LogRecord {
            ts,
            level: self.level,
            computer,
            event: self.name,
            peer: self.peer,
            fields: self.fields,
        }
    }
}

/// `fields` from a JSON object literal.
fn fields(value: Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

fn with_app(mut map: Map<String, Value>, app: Option<&ForegroundApp>) -> Map<String, Value> {
    if let Some(app) = app {
        map.insert("app".into(), json!(app.name));
        map.insert("app_admin".into(), json!(app.admin));
    }
    map
}

fn os_name(os: Os) -> Value {
    serde_json::to_value(os).expect("os serializes")
}

pub fn header(
    computer: ComputerId,
    name: &str,
    context: &super::Context,
    log_path: &Path,
) -> Event {
    let mut map = Map::new();
    map.insert("mouser_version".into(), json!(context.mouser_version));
    map.insert("os".into(), os_name(context.os));
    map.insert("os_version".into(), json!(context.os_version));
    map.insert("computer_id".into(), json!(computer.to_string()));
    map.insert("computer_name".into(), json!(name));
    if let Some((id, desk_name)) = &context.desk {
        map.insert("desk_id".into(), json!(id.to_string()));
        map.insert("desk_name".into(), json!(desk_name));
    }
    map.insert(
        "mode".into(),
        serde_json::to_value(context.mode).expect("mode serializes"),
    );
    map.insert("log_path".into(), json!(log_path.to_string_lossy()));
    Event::new(LogLevel::Info, "log.header", None, Some(map))
}

pub fn logging_enabled(path: &Path) -> Event {
    Event::new(
        LogLevel::Info,
        "logging.enabled",
        None,
        fields(json!({"path": path.to_string_lossy()})),
    )
}

pub fn logging_disabled(path: &Path) -> Event {
    Event::new(
        LogLevel::Info,
        "logging.disabled",
        None,
        fields(json!({"path": path.to_string_lossy()})),
    )
}

/// Control moved from `from` to `to`; `app` is the application in front on
/// this computer, given when this computer is `to`.
pub fn control_changed(
    from: ComputerId,
    to: ComputerId,
    how: ControlHow,
    app: Option<&ForegroundApp>,
) -> Event {
    let map = with_app(
        fields(json!({"from": from.to_string(), "to": to.to_string(), "how": how.as_str()}))
            .expect("an object"),
        app,
    );
    Event::new(LogLevel::Info, "control.changed", None, Some(map))
}

pub fn pointer_hidden(reason: PointerHidden) -> Event {
    Event::new(
        LogLevel::Info,
        "pointer.hidden",
        None,
        fields(json!({"reason": reason.as_str()})),
    )
}

pub fn pointer_shown(reason: PointerShown) -> Event {
    Event::new(
        LogLevel::Info,
        "pointer.shown",
        None,
        fields(json!({"reason": reason.as_str()})),
    )
}

/// The operating system refused an input of `kind`; `peer` is the controller
/// it came from, if there is one.
pub fn input_refused(
    peer: Option<ComputerId>,
    kind: InputKind,
    app: Option<&ForegroundApp>,
) -> Event {
    let map = with_app(
        fields(json!({"kind": kind.as_str()})).expect("an object"),
        app,
    );
    Event::new(LogLevel::Warn, "input.refused", peer, Some(map))
}

pub fn input_refused_run_ended(peer: Option<ComputerId>, kind: InputKind, count: u32) -> Event {
    Event::new(
        LogLevel::Info,
        "input.refused_run_ended",
        peer,
        fields(json!({"kind": kind.as_str(), "count": count})),
    )
}

pub fn peer_connected(peer: ComputerId, version: &str, os: Os) -> Event {
    Event::new(
        LogLevel::Info,
        "peer.connected",
        Some(peer),
        fields(json!({"version": version, "os": os_name(os)})),
    )
}

pub fn peer_disconnected(peer: ComputerId, reason: LinkLossReason) -> Event {
    Event::new(
        LogLevel::Info,
        "peer.disconnected",
        Some(peer),
        fields(json!({"reason": reason.as_str()})),
    )
}

pub fn wire_unknown(peer: ComputerId, variant: u32) -> Event {
    Event::new(
        LogLevel::Warn,
        "wire.unknown",
        Some(peer),
        fields(json!({"variant": variant})),
    )
}

/// A fault at `at` (a thread or unit of work), found at `location`
/// (`file:line`).
pub fn fault(at: &str, location: &str, message: &str) -> Event {
    Event::new(
        LogLevel::Error,
        "fault",
        None,
        fields(json!({"where": at, "location": location, "message": message})),
    )
}

pub fn fault_recovered(at: &str, action: RecoveryAction) -> Event {
    Event::new(
        LogLevel::Warn,
        "fault.recovered",
        None,
        fields(json!({"where": at, "action": action.as_str()})),
    )
}

pub fn display_changed(computer: ComputerId, display: &str, change: DisplayChange) -> Event {
    Event::new(
        LogLevel::Info,
        "display.changed",
        None,
        fields(json!({
            "computer": computer.to_string(),
            "display": display,
            "change": change.as_str(),
        })),
    )
}

pub fn layout_changed(by: ComputerId, computer: ComputerId, change: LayoutChange) -> Event {
    Event::new(
        LogLevel::Info,
        "layout.changed",
        None,
        fields(json!({
            "by": by.to_string(),
            "computer": computer.to_string(),
            "change": change.as_str(),
        })),
    )
}

pub fn setting_changed(change: &SettingChange) -> Event {
    let origin = match change.origin {
        SettingOrigin::App => "app",
        SettingOrigin::Cli => "cli",
        SettingOrigin::File => "file",
        SettingOrigin::DeskMate => "desk_mate",
    };
    let mut map = fields(json!({
        "key": change.key,
        "old": change.old,
        "new": change.new,
        "origin": origin,
    }))
    .expect("an object");
    if let Some(from) = change.from {
        map.insert("from".into(), json!(from.to_string()));
    }
    Event::new(LogLevel::Info, "setting.changed", None, Some(map))
}

/// The representations of a clipboard offer by kind and size (bytes; the
/// number of items for files), never what they hold.
fn clipboard(
    name: &str,
    peer: ComputerId,
    from: ComputerId,
    to: ComputerId,
    reps: &[(ClipboardKind, u64)],
) -> Event {
    let reps: Vec<Value> = reps
        .iter()
        .map(|(kind, size)| {
            let kind = match kind {
                ClipboardKind::TextPlain => "text",
                ClipboardKind::TextHtml => "html",
                ClipboardKind::TextRtf => "rtf",
                ClipboardKind::ImagePng => "png",
                ClipboardKind::Files => "files",
            };
            json!({"kind": kind, "size": size})
        })
        .collect();
    Event::new(
        LogLevel::Info,
        name,
        Some(peer),
        fields(json!({"from": from.to_string(), "to": to.to_string(), "reps": reps})),
    )
}

pub fn clipboard_sent(
    peer: ComputerId,
    from: ComputerId,
    to: ComputerId,
    reps: &[(ClipboardKind, u64)],
) -> Event {
    clipboard("clipboard.sent", peer, from, to, reps)
}

pub fn clipboard_received(
    peer: ComputerId,
    from: ComputerId,
    to: ComputerId,
    reps: &[(ClipboardKind, u64)],
) -> Event {
    clipboard("clipboard.received", peer, from, to, reps)
}

pub fn logs_collected(
    path: &Path,
    included: usize,
    missing: &[(ComputerId, BundleReason)],
) -> Event {
    let missing: Vec<Value> = missing
        .iter()
        .map(|(computer, reason)| {
            json!({
                "computer": computer.to_string(),
                "reason": serde_json::to_value(reason).expect("reason serializes"),
            })
        })
        .collect();
    Event::new(
        LogLevel::Info,
        "logs.collected",
        None,
        fields(json!({"path": path.to_string_lossy(), "included": included, "missing": missing})),
    )
}

pub fn elevation_restart(outcome: ElevationOutcome) -> Event {
    Event::new(
        LogLevel::Info,
        "elevation.restart",
        None,
        fields(json!({"outcome": outcome.as_str()})),
    )
}

pub fn service_session(session: u32) -> Event {
    Event::new(
        LogLevel::Info,
        "service.session",
        None,
        fields(json!({"session": session})),
    )
}

pub fn service_engine_started(pid: u32, session: u32) -> Event {
    Event::new(
        LogLevel::Info,
        "service.engine_started",
        None,
        fields(json!({"pid": pid, "session": session})),
    )
}

pub fn service_engine_exited(code: u32) -> Event {
    Event::new(
        LogLevel::Info,
        "service.engine_exited",
        None,
        fields(json!({"code": code})),
    )
}

/// The outcome of a Ctrl-Alt-Del request; a failure's reason is not logged.
pub fn service_sas(result: &SasResult) -> Event {
    let result = match result {
        SasResult::Sent => "sent",
        SasResult::PolicyBlocked => "policy_blocked",
        SasResult::Failed(_) => "failed",
    };
    Event::new(
        LogLevel::Info,
        "service.sas",
        None,
        fields(json!({"result": result})),
    )
}
fn problem(name: &str, problem: &Problem) -> Event {
    Event::new(
        LogLevel::Warn,
        name,
        Some(problem.computer),
        fields(json!({
            "kind": serde_json::to_value(problem.kind).expect("kind serializes"),
            "problem_id": problem.problem_id,
            "computer": problem.computer.to_string(),
            "message": problem.message,
        })),
    )
}

pub fn problem_raised(raised: &Problem) -> Event {
    problem("problem.raised", raised)
}

pub fn problem_cleared(cleared: &Problem) -> Event {
    problem("problem.cleared", cleared)
}

pub fn config_rejected(reason: &str) -> Event {
    Event::new(
        LogLevel::Warn,
        "config.rejected",
        None,
        fields(json!({"reason": reason})),
    )
}

pub fn update_available(peer: Option<ComputerId>, version: &str) -> Event {
    Event::new(
        LogLevel::Info,
        "update.available",
        peer,
        fields(json!({"version": version})),
    )
}

pub fn update_failed(message: &str) -> Event {
    Event::new(
        LogLevel::Warn,
        "update.failed",
        None,
        fields(json!({"message": message})),
    )
}

pub fn update_check_failed(message: &str) -> Event {
    Event::new(
        LogLevel::Warn,
        "update.check_failed",
        None,
        fields(json!({"message": message})),
    )
}

/// What a platform backend reports through its diagnostics sink: a
/// `capture.*` event of the catalog with the fields it names.
pub fn platform_diagnostic(
    level: LogLevel,
    event: &str,
    peer: Option<ComputerId>,
    fields: Option<Map<String, Value>>,
) -> Event {
    assert!(
        event.starts_with("capture."),
        "a platform backend reports capture.* events, not {event}"
    );
    Event::new(level, event, peer, fields)
}
