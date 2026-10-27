use crate::capture::{State, handle};
use crate::foreground::{Probe, SystemProbe, foreground_app};
use core_graphics::display::CGPoint;
use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::{ForegroundApp, LocalInput};
use std::sync::Arc;
use std::sync::mpsc::channel;

struct Fake(Option<&'static str>);

impl Probe for Fake {
    fn frontmost_name(&self) -> Option<String> {
        self.0.map(str::to_owned)
    }
}

#[test]
fn the_frontmost_applications_localized_name_is_reported_without_elevation() {
    assert_eq!(
        foreground_app(&Fake(Some("Safari"))),
        Some(ForegroundApp {
            name: "Safari".into(),
            admin: false,
            outranks_mouser: false,
        })
    );
}

#[test]
fn no_frontmost_application_is_none() {
    assert_eq!(foreground_app(&Fake(None)), None);
}

#[test]
fn system_probe_reads_the_frontmost_application() {
    // Read-only NSWorkspace query; a headless runner may have none.
    if let Some(name) = SystemProbe.frontmost_name() {
        assert!(!name.is_empty());
    }
}

#[test]
fn capture_lost_is_never_emitted() {
    let state = State::new(Arc::default());
    let (tx, rx) = channel();
    *state.tx.lock().unwrap() = Some(tx);
    let source = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
    let moved = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::MouseMoved,
        CGPoint::new(5.0, 5.0),
        CGMouseButton::Left,
    )
    .unwrap();
    let key = CGEvent::new_keyboard_event(source, 0, true).unwrap();
    // Mouse move, key down, and the tap-disabled-by-timeout / by-user types.
    for (ty, ev) in [
        (5, &moved),
        (10, &key),
        (0xFFFF_FFFE, &moved),
        (0xFFFF_FFFF, &moved),
    ] {
        handle(&state, ty, ev);
    }
    let emitted: Vec<LocalInput> = rx.try_iter().collect();
    assert!(!emitted.is_empty());
    assert!(
        !emitted
            .iter()
            .any(|i| matches!(i, LocalInput::CaptureLost(_)))
    );
}
