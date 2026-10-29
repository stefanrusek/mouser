use crate::diag::*;
use mouser_platform::CaptureLossReason;
use std::sync::{Arc, Mutex};

fn start() -> DiagEvent {
    DiagEvent::CaptureStart { unthrottled: true }
}

#[test]
fn diagnostics_reach_the_sink_while_one_is_set() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    note(start());
    set_diagnostics(Some(Arc::new(move |level, event, fields| {
        // Other tests' captures may write meanwhile.
        if event == "capture.start" {
            sink.lock()
                .unwrap()
                .push((level, event.to_string(), fields));
        }
    })));
    note(start());
    set_diagnostics(None);
    note(start());
    let heard = heard.lock().unwrap();
    assert!(!heard.is_empty(), "the line written while the sink was set");
    assert!(heard.iter().all(|line| *line
        == (
            DiagLevel::Debug,
            "capture.start".to_string(),
            vec![("unthrottled", DiagValue::Bool(true))]
        )));
}

#[test]
fn each_line_has_its_catalog_name_level_and_typed_fields() {
    let lost = DiagEvent::CaptureLost {
        reason: CaptureLossReason::ElevatedForeground,
        foreground: Some("Taskmgr.exe".into()),
    };
    assert_eq!(lost.name(), "capture.lost");
    assert_eq!(lost.level(), DiagLevel::Warn);
    assert_eq!(
        lost.fields(),
        [
            ("reason", DiagValue::Str("elevated_foreground".into())),
            ("foreground", DiagValue::Str("Taskmgr.exe".into()))
        ]
    );
    let shown = DiagEvent::PointerShownByGuard;
    assert_eq!(
        (shown.name(), shown.level()),
        ("pointer.shown", DiagLevel::Info)
    );
    let reinstall = DiagEvent::CaptureReinstall {
        hook: DiagHook::Keyboard,
        ok: false,
    };
    assert_eq!(reinstall.name(), "capture.reinstall");
    assert_eq!(
        reinstall.fields(),
        [
            ("hook", DiagValue::Str("keyboard".into())),
            ("ok", DiagValue::Bool(false))
        ]
    );
    let thread = DiagEvent::CaptureHookThread {
        time_critical: true,
    };
    assert_eq!(
        (thread.name(), thread.level(), thread.fields()),
        (
            "capture.hook_thread",
            DiagLevel::Debug,
            vec![("time_critical", DiagValue::Bool(true))]
        )
    );
}

#[test]
fn a_probe_line_writes_only_what_it_has() {
    let missed = DiagEvent::CaptureProbe(Probe {
        outcome: ProbeOutcome::Missed,
        hook: Some(DiagHook::Mouse),
        misses: Some(2),
        reason: None,
        foreground: Some("game.exe".into()),
    });
    assert_eq!(missed.name(), "capture.probe");
    assert_eq!(
        missed.fields(),
        [
            ("outcome", DiagValue::Str("missed".into())),
            ("hook", DiagValue::Str("mouse".into())),
            ("misses", DiagValue::Int(2)),
            ("foreground", DiagValue::Str("game.exe".into()))
        ]
    );
    let skipped = Probe {
        outcome: ProbeOutcome::Skipped,
        hook: None,
        misses: None,
        reason: Some("locked"),
        foreground: None,
    };
    assert_eq!(
        skipped.fields(),
        [
            ("outcome", DiagValue::Str("skipped".into())),
            ("reason", DiagValue::Str("locked".into()))
        ]
    );
}

#[test]
fn values_read_as_they_are_written() {
    assert_eq!(DiagValue::Str("game.exe".into()).to_string(), "game.exe");
    assert_eq!(DiagValue::Bool(false).to_string(), "false");
    assert_eq!(DiagValue::Int(3).to_string(), "3");
}
