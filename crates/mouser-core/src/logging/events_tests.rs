//! One typed constructor per row of the log-events catalog
//! (contracts/log-events.md): each produces exactly the catalog's fields and
//! enum values, and nothing else can reach the log (FR-003).

use std::path::Path;

use mouser_platform::ForegroundApp;
use mouser_proto::control::{BundleReason, Problem, ProblemKind, SettingChange, SettingOrigin};
use mouser_proto::ids::ComputerId;
use mouser_proto::log::{LogComputer, LogLevel};
use mouser_proto::model::Os;
use mouser_proto::wire::{ClipboardKind, ComputerMode};
use serde_json::{Value, json};

use super::Context;
use super::events::*;
use crate::session::close_reason::LinkLossReason;

const ME: ComputerId = ComputerId([7; 16]);
const A: ComputerId = ComputerId([1; 16]);
const B: ComputerId = ComputerId([2; 16]);

/// The v2 catalog schema, with the record schema it refers to registered.
pub(crate) fn validator() -> jsonschema::Validator {
    let record: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/log-record.schema.json"
    ))
    .unwrap();
    let events: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/log-events.schema.json"
    ))
    .unwrap();
    let registry = jsonschema::Registry::new()
        .add("https://mouser.local/schemas/log-record.v1.json", record)
        .unwrap()
        .prepare()
        .unwrap();
    jsonschema::options()
        .with_registry(&registry)
        .build(&events)
        .unwrap()
}

fn record(event: Event) -> Value {
    serde_json::to_value(event.into_record(
        LogComputer {
            id: ME,
            name: "Alpha".into(),
        },
        "2026-10-27T00:00:00.000Z".into(),
    ))
    .unwrap()
}

/// The record `event` becomes, which must satisfy the closed catalog.
fn checked(event: Event) -> Value {
    let value = record(event);
    if let Err(e) = validator().validate(&value) {
        panic!("{e}: {value}");
    }
    value
}

fn expect(event: Event, name: &str, level: &str, peer: Option<ComputerId>, fields: Value) {
    let value = checked(event);
    assert_eq!(value["event"], name);
    assert_eq!(value["level"], level);
    assert_eq!(
        value.get("peer").cloned(),
        peer.map(|p| Value::String(p.to_string()))
    );
    assert_eq!(value.get("fields").cloned(), Some(fields), "{value}");
}

fn app(name: &str, admin: bool) -> ForegroundApp {
    ForegroundApp {
        name: name.into(),
        admin,
        outranks_mouser: false,
    }
}

fn id(c: ComputerId) -> String {
    c.to_string()
}

#[test]
fn log_header_states_version_os_computer_desk_mode_and_path() {
    let context = Context {
        mouser_version: "0.3.0".into(),
        os: Os::Windows,
        os_version: "11 23H2".into(),
        mode: ComputerMode::PortableElevated,
        desk: Some((mouser_proto::ids::DeskId([3; 16]), "Home office".into())),
    };
    expect(
        header(ME, "Alpha", &context, Path::new("/logs/mouser.log")),
        "log.header",
        "info",
        None,
        json!({
            "mouser_version": "0.3.0", "os": "windows", "os_version": "11 23H2",
            "computer_id": id(ME), "computer_name": "Alpha",
            "desk_id": mouser_proto::ids::DeskId([3; 16]).to_string(),
            "desk_name": "Home office", "mode": "portable_elevated",
            "log_path": "/logs/mouser.log"
        }),
    );
    let alone = Context {
        desk: None,
        ..context
    };
    let value = checked(header(ME, "Alpha", &alone, Path::new("/logs/mouser.log")));
    assert!(value["fields"].get("desk_id").is_none());
    assert!(value["fields"].get("desk_name").is_none());
}

#[test]
fn logging_on_and_off_carry_the_path() {
    let path = Path::new("/logs/mouser.log");
    expect(
        logging_enabled(path),
        "logging.enabled",
        "info",
        None,
        json!({"path": "/logs/mouser.log"}),
    );
    expect(
        logging_disabled(path),
        "logging.disabled",
        "info",
        None,
        json!({"path": "/logs/mouser.log"}),
    );
}

#[test]
fn control_changed_names_the_computers_how_and_the_application_in_front() {
    expect(
        control_changed(ME, A, ControlHow::Crossing, None),
        "control.changed",
        "info",
        None,
        json!({"from": id(ME), "to": id(A), "how": "crossing"}),
    );
    expect(
        control_changed(A, ME, ControlHow::Entered, Some(&app("regedit.exe", true))),
        "control.changed",
        "info",
        None,
        json!({"from": id(A), "to": id(ME), "how": "entered", "app": "regedit.exe", "app_admin": true}),
    );
    let hows: Vec<_> = ControlHow::ALL.iter().map(|h| h.as_str()).collect();
    assert_eq!(
        hows,
        [
            "crossing",
            "panic",
            "quickjump",
            "pause",
            "link_lost",
            "capture_lost",
            "elevated_app",
            "fault",
            "quit",
            "entered"
        ]
    );
    for how in ControlHow::ALL {
        checked(control_changed(ME, A, *how, None));
    }
}

#[test]
fn pointer_records_say_which_and_why() {
    expect(
        pointer_hidden(PointerHidden::CrossingOut),
        "pointer.hidden",
        "info",
        None,
        json!({"reason": "crossing_out"}),
    );
    let hidden: Vec<_> = PointerHidden::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(hidden, ["crossing_out", "quickjump"]);
    let shown: Vec<_> = PointerShown::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(
        shown,
        [
            "returned",
            "capture_lost",
            "fault",
            "quit",
            "startup_recovery",
            "guard"
        ]
    );
    for reason in PointerHidden::ALL {
        checked(pointer_hidden(*reason));
    }
    for reason in PointerShown::ALL {
        expect(
            pointer_shown(*reason),
            "pointer.shown",
            "info",
            None,
            json!({"reason": reason.as_str()}),
        );
    }
}

#[test]
fn refused_input_and_the_end_of_a_run_name_the_controller_kind_and_application() {
    expect(
        input_refused(Some(A), InputKind::Key, Some(&app("notepad.exe", false))),
        "input.refused",
        "warn",
        Some(A),
        json!({"kind": "key", "app": "notepad.exe", "app_admin": false}),
    );
    expect(
        input_refused(None, InputKind::Grab, None),
        "input.refused",
        "warn",
        None,
        json!({"kind": "grab"}),
    );
    expect(
        input_refused_run_ended(Some(A), InputKind::Scroll, 3),
        "input.refused_run_ended",
        "info",
        Some(A),
        json!({"kind": "scroll", "count": 3}),
    );
    let kinds: Vec<_> = InputKind::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(kinds, ["key", "button", "motion", "scroll", "grab", "warp"]);
    for kind in InputKind::ALL {
        checked(input_refused(Some(A), *kind, None));
        checked(input_refused_run_ended(None, *kind, 1));
    }
}

#[test]
fn peer_records_carry_the_peer_version_os_and_reason() {
    expect(
        peer_connected(A, "0.2.8", Os::Macos),
        "peer.connected",
        "info",
        Some(A),
        json!({"version": "0.2.8", "os": "macos"}),
    );
    for reason in LinkLossReason::ALL {
        expect(
            peer_disconnected(A, *reason),
            "peer.disconnected",
            "info",
            Some(A),
            json!({"reason": reason.as_str()}),
        );
    }
    let reasons: Vec<_> = LinkLossReason::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(
        reasons,
        [
            "peer_left",
            "peer_quit",
            "removed",
            "replaced",
            "unreachable",
            "incompatible",
            "protocol",
            "fault_here",
            "quit_here"
        ]
    );
    expect(
        wire_unknown(A, 40),
        "wire.unknown",
        "warn",
        Some(A),
        json!({"variant": 40}),
    );
}

#[test]
fn fault_and_recovery_records() {
    expect(
        fault("read_loop", "crates/x/src/y.rs:12", "boom"),
        "fault",
        "error",
        None,
        json!({"where": "read_loop", "location": "crates/x/src/y.rs:12", "message": "boom"}),
    );
    let actions: Vec<_> = RecoveryAction::ALL.iter().map(|a| a.as_str()).collect();
    assert_eq!(actions, ["returned_local", "closed_link", "continued"]);
    for action in RecoveryAction::ALL {
        expect(
            fault_recovered("reconcile", *action),
            "fault.recovered",
            "warn",
            None,
            json!({"where": "reconcile", "action": action.as_str()}),
        );
    }
}

#[test]
fn display_and_layout_changes() {
    expect(
        display_changed(B, "LG UltraFine", DisplayChange::Added),
        "display.changed",
        "info",
        None,
        json!({"computer": id(B), "display": "LG UltraFine", "change": "added"}),
    );
    let changes: Vec<_> = DisplayChange::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(changes, ["added", "removed", "rearranged", "resized"]);
    for change in DisplayChange::ALL {
        checked(display_changed(B, "d", *change));
    }
    expect(
        layout_changed(A, B, LayoutChange::Moved),
        "layout.changed",
        "info",
        None,
        json!({"by": id(A), "computer": id(B), "change": "moved"}),
    );
    let changes: Vec<_> = LayoutChange::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        changes,
        [
            "moved",
            "display_removed",
            "display_included",
            "kept_placement",
            "auto_placed"
        ]
    );
    for change in LayoutChange::ALL {
        checked(layout_changed(A, B, *change));
    }
}

#[test]
fn setting_changes_carry_old_new_origin_and_the_desk_mate() {
    let change = |origin, from| SettingChange {
        key: "crossing.block_fullscreen".into(),
        old: json!(true),
        new: json!(false),
        origin,
        from,
    };
    expect(
        setting_changed(&change(SettingOrigin::Cli, None)),
        "setting.changed",
        "info",
        None,
        json!({"key": "crossing.block_fullscreen", "old": true, "new": false, "origin": "cli"}),
    );
    expect(
        setting_changed(&change(SettingOrigin::DeskMate, Some(A))),
        "setting.changed",
        "info",
        None,
        json!({"key": "crossing.block_fullscreen", "old": true, "new": false, "origin": "desk_mate", "from": id(A)}),
    );
    for origin in [SettingOrigin::App, SettingOrigin::Cli, SettingOrigin::File] {
        checked(setting_changed(&change(origin, None)));
    }
}

#[test]
fn clipboard_records_have_kinds_and_sizes_and_never_content() {
    let reps = [
        (ClipboardKind::TextPlain, 5),
        (ClipboardKind::TextHtml, 20),
        (ClipboardKind::TextRtf, 30),
        (ClipboardKind::ImagePng, 400),
        (ClipboardKind::Files, 2),
    ];
    let fields = json!({
        "from": id(A), "to": id(B),
        "reps": [
            {"kind": "text", "size": 5}, {"kind": "html", "size": 20},
            {"kind": "rtf", "size": 30}, {"kind": "png", "size": 400},
            {"kind": "files", "size": 2}
        ]
    });
    expect(
        clipboard_sent(B, A, B, &reps),
        "clipboard.sent",
        "info",
        Some(B),
        fields.clone(),
    );
    expect(
        clipboard_received(A, A, B, &reps),
        "clipboard.received",
        "info",
        Some(A),
        fields,
    );
}

#[test]
fn logs_collected_lists_what_is_missing_and_why() {
    expect(
        logs_collected(
            Path::new("/Downloads/mouser-logs.zip"),
            2,
            &[
                (A, BundleReason::Offline),
                (B, BundleReason::LoggingOff),
                (ComputerId([3; 16]), BundleReason::OlderVersion),
            ],
        ),
        "logs.collected",
        "info",
        None,
        json!({
            "path": "/Downloads/mouser-logs.zip", "included": 2,
            "missing": [
                {"computer": id(A), "reason": "offline"},
                {"computer": id(B), "reason": "logging_off"},
                {"computer": id(ComputerId([3; 16])), "reason": "older_version"}
            ]
        }),
    );
}

#[test]
fn elevation_restart_outcomes() {
    let outcomes: Vec<_> = ElevationOutcome::ALL.iter().map(|o| o.as_str()).collect();
    assert_eq!(outcomes, ["prompted", "declined", "handed_over"]);
    for outcome in ElevationOutcome::ALL {
        expect(
            elevation_restart(*outcome),
            "elevation.restart",
            "info",
            None,
            json!({"outcome": outcome.as_str()}),
        );
    }
}

#[test]
fn problem_records_carry_kind_id_computer_and_message() {
    let problem = Problem {
        problem_id: "secure_input:Terminal".into(),
        kind: ProblemKind::SecureInput,
        computer: A,
        message: "Terminal holds Secure Input".into(),
        app: None,
        fix: None,
        raised_at: "2026-10-27T00:00:00.000Z".into(),
        cleared_at: None,
    };
    let fields = json!({
        "kind": "secure_input", "problem_id": "secure_input:Terminal",
        "computer": id(A), "message": "Terminal holds Secure Input"
    });
    expect(
        problem_raised(&problem),
        "problem.raised",
        "warn",
        Some(A),
        fields.clone(),
    );
    expect(
        problem_cleared(&problem),
        "problem.cleared",
        "warn",
        Some(A),
        fields,
    );
}

#[test]
fn the_001_events_keep_their_shape() {
    expect(
        config_rejected("hotkeys.panic: nope"),
        "config.rejected",
        "warn",
        None,
        json!({"reason": "hotkeys.panic: nope"}),
    );
    expect(
        update_available(None, "0.3.1"),
        "update.available",
        "info",
        None,
        json!({"version": "0.3.1"}),
    );
    expect(
        update_available(Some(A), "0.3.1"),
        "update.available",
        "info",
        Some(A),
        json!({"version": "0.3.1"}),
    );
    expect(
        update_failed("no bundle"),
        "update.failed",
        "warn",
        None,
        json!({"message": "no bundle"}),
    );
    expect(
        update_check_failed("offline"),
        "update.check_failed",
        "warn",
        None,
        json!({"message": "offline"}),
    );
}

#[test]
fn a_platform_backends_capture_diagnostics_are_debug_records() {
    let fields = json!({"outcome": "missed", "hook": "mouse"})
        .as_object()
        .cloned();
    let value = checked(platform_diagnostic(
        LogLevel::Debug,
        "capture.probe",
        None,
        fields,
    ));
    assert_eq!(value["level"], "debug");
    assert_eq!(
        value["fields"],
        json!({"outcome": "missed", "hook": "mouse"})
    );
}

#[test]
#[should_panic(expected = "capture.")]
fn only_capture_diagnostics_come_from_a_platform_backend() {
    platform_diagnostic(LogLevel::Debug, "control.changed", None, None);
}

#[test]
fn no_constructor_takes_a_free_form_map_or_value() {
    let source = include_str!("events.rs");
    for item in source.split("pub fn ").skip(1) {
        let signature = item.split('{').next().unwrap();
        if signature.starts_with("platform_diagnostic") {
            continue;
        }
        for banned in ["Map<", "Value", "HashMap", "BTreeMap"] {
            assert!(!signature.contains(banned), "{signature}");
        }
    }
}

#[test]
fn the_service_writes_its_session_engine_and_ctrl_alt_del_events() {
    use mouser_proto::service_ipc::SasResult;
    expect(
        service_session(2),
        "service.session",
        "info",
        None,
        json!({"session": 2}),
    );
    expect(
        service_engine_started(4321, 2),
        "service.engine_started",
        "info",
        None,
        json!({"pid": 4321, "session": 2}),
    );
    expect(
        service_engine_exited(0xC000_0005),
        "service.engine_exited",
        "info",
        None,
        json!({"code": 0xC000_0005u32}),
    );
    for (result, text) in [
        (SasResult::Sent, "sent"),
        (SasResult::PolicyBlocked, "policy_blocked"),
        (SasResult::Failed("access is denied".into()), "failed"),
    ] {
        expect(
            service_sas(&result),
            "service.sas",
            "info",
            None,
            json!({"result": text}),
        );
    }
}
