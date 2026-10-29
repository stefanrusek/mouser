//! Contract test (consumer side) for contracts/log-events.md: every line
//! the Windows backend's diagnostics can write, each event with each value
//! its fields can take, set and not set, wrapped in a log record as the app
//! writes it, validates against the closed catalog
//! (`log-events.schema.json`, with `log-record.schema.json` registered as
//! the resource it refers to).

#![cfg(windows)]

use jsonschema::{Registry, Validator};
use mouser_platform::CaptureLossReason;
use mouser_platform_windows::{DiagEvent, DiagHook, DiagLevel, DiagValue, Probe, ProbeOutcome};
use serde_json::{Map, Value, json};

fn events_validator() -> Validator {
    let record: Value = serde_json::from_str(include_str!(
        "../../mouser-proto/schemas/log-record.schema.json"
    ))
    .unwrap();
    let events: Value = serde_json::from_str(include_str!(
        "../../mouser-proto/schemas/log-events.schema.json"
    ))
    .unwrap();
    let registry = Registry::new()
        .add("https://mouser.local/schemas/log-record.v1.json", record)
        .unwrap()
        .prepare()
        .unwrap();
    jsonschema::options()
        .with_registry(&registry)
        .build(&events)
        .unwrap()
}

/// The line as the app's sink writes it: a log record of this computer.
fn record(level: DiagLevel, event: &str, fields: Vec<(&'static str, DiagValue)>) -> Value {
    let level = match level {
        DiagLevel::Debug => "debug",
        DiagLevel::Info => "info",
        DiagLevel::Warn => "warn",
    };
    let fields: Map<String, Value> = fields
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
    json!({
        "ts": "2026-10-29T12:00:00Z",
        "level": level,
        "computer": { "id": "0123456789abcdef0123456789abcdef", "name": "PC" },
        "event": event,
        "fields": fields,
    })
}

fn probe(outcome: ProbeOutcome) -> Probe {
    Probe {
        outcome,
        hook: None,
        misses: None,
        reason: None,
        foreground: None,
    }
}

/// Every line the backend can write. The match below names every variant,
/// so a new one does not compile until it is listed here.
fn every_line() -> Vec<DiagEvent> {
    let mut all = vec![DiagEvent::PointerShownByGuard];
    for reason in [
        CaptureLossReason::ElevatedForeground,
        CaptureLossReason::OtherDesktop,
        CaptureLossReason::HooksSilent,
    ] {
        for foreground in [None, Some("Taskmgr.exe".to_string())] {
            all.push(DiagEvent::CaptureLost { reason, foreground });
        }
    }
    for on in [false, true] {
        all.push(DiagEvent::CaptureStart { unthrottled: on });
        all.push(DiagEvent::CaptureHookThread { time_critical: on });
        for hook in [DiagHook::Mouse, DiagHook::Keyboard] {
            all.push(DiagEvent::CaptureReinstall { hook, ok: on });
        }
    }
    for foreground in [None, Some("game.exe".to_string())] {
        let with = |mut p: Probe| {
            p.foreground = foreground.clone();
            DiagEvent::CaptureProbe(p)
        };
        for outcome in [
            ProbeOutcome::Seen,
            ProbeOutcome::NotSent,
            ProbeOutcome::Live,
        ] {
            all.push(with(probe(outcome)));
        }
        for hook in [DiagHook::Mouse, DiagHook::Keyboard] {
            for outcome in [ProbeOutcome::Unseen, ProbeOutcome::Late] {
                all.push(with(Probe {
                    hook: Some(hook),
                    ..probe(outcome)
                }));
            }
            all.push(with(Probe {
                hook: Some(hook),
                misses: Some(2),
                ..probe(ProbeOutcome::Missed)
            }));
        }
        // The watchdog's reasons not to probe.
        for reason in ["locked", "other_desktop", "elevated_foreground", "idle"] {
            all.push(with(Probe {
                reason: Some(reason),
                ..probe(ProbeOutcome::Skipped)
            }));
        }
    }
    for line in &all {
        match line {
            DiagEvent::PointerShownByGuard
            | DiagEvent::CaptureLost { .. }
            | DiagEvent::CaptureStart { .. }
            | DiagEvent::CaptureHookThread { .. }
            | DiagEvent::CaptureReinstall { .. }
            | DiagEvent::CaptureProbe(_) => {}
        }
    }
    all
}

#[test]
fn every_line_the_backend_can_write_is_in_the_catalog() {
    let validator = events_validator();
    for line in every_line() {
        let record = record(line.level(), line.name(), line.fields());
        if let Err(e) = validator.validate(&record) {
            panic!("{record} is not in the catalog: {e}");
        }
    }
}

#[test]
fn the_catalog_checks_the_fields_it_is_given() {
    // A boolean written as text, as the backend once did, is refused.
    let validator = events_validator();
    let text = record(
        DiagLevel::Debug,
        "capture.reinstall",
        vec![("ok", DiagValue::Str("true".into()))],
    );
    assert!(validator.validate(&text).is_err());
}
