use std::sync::Arc;

use mouser_platform::Os as HostOs;
use mouser_proto::control::{EngineState, Status};
use mouser_proto::model::Os;
use mouser_testkit::VirtualPlatform;

use crate::notify::Notifier;
use crate::support_tests::{FakeNotifier, fixture, fixture_on, status_of};
use crate::tray::{Action, Appearance, action, view};
use crate::tray_sas::{SEND_CAD_ID, TITLE, entry, report, send, shows_send_cad};

fn windows_status(mut status: Status, active_remote: bool) -> Status {
    let mut other = status.computers[0].clone();
    other.id = mouser_proto::ids::ComputerId([9; 16]);
    other.name = "Studio".into();
    other.os = Os::Windows;
    if active_remote {
        status.active_computer = other.id;
    } else {
        status.computers[0].os = Os::Windows;
    }
    status.computers.push(other);
    status
}

#[tokio::test(flavor = "multi_thread")]
async fn the_line_shows_only_while_a_windows_computer_is_active() {
    let f = fixture().await;
    let mut status = status_of(&f.host).await;
    status.computers[0].os = Os::Macos;
    assert!(!shows_send_cad(&status));
    assert!(entry(&status).is_none());

    // The pointer is on a Windows computer.
    let remote = windows_status(status.clone(), true);
    assert!(shows_send_cad(&remote));
    let line = entry(&remote).unwrap();
    assert_eq!(line.id, Some(SEND_CAD_ID));
    assert_eq!(line.label, "Send Ctrl-Alt-Del");

    // The pointer is at home and this computer is Windows.
    let home = windows_status(status.clone(), false);
    assert!(shows_send_cad(&home));

    // The pointer is on a computer that is not Windows.
    let mut elsewhere = windows_status(status, true);
    elsewhere.computers[1].os = Os::Linux;
    assert!(!shows_send_cad(&elsewhere));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tray_menu_carries_the_line_before_quit() {
    let f = fixture().await;
    let mut status = status_of(&f.host).await;
    let plain = view(&status, HostOs::Windows, Appearance::Light, None);
    assert!(plain.entries.iter().all(|e| e.id != Some(SEND_CAD_ID)));

    status = windows_status(status, true);
    status.state = EngineState::Active;
    let with = view(&status, HostOs::Windows, Appearance::Light, None);
    let ids: Vec<_> = with.entries.iter().map(|e| e.id).collect();
    let at = ids.iter().position(|id| *id == Some(SEND_CAD_ID)).unwrap();
    assert_eq!(ids[at + 1], Some(crate::tray::QUIT_ID));
    assert_eq!(action(SEND_CAD_ID), Some(Action::SendCad));
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_to_a_computer_that_is_not_windows_returns_the_engines_message() {
    let f = fixture().await;
    let err = send(&f.host.control()).await.unwrap_err();
    assert_eq!(err, "Ctrl-Alt-Del cannot be sent to Desk Mac.");
}

#[tokio::test(flavor = "multi_thread")]
async fn sending_to_this_windows_computer_succeeds() {
    let platform = VirtualPlatform::new();
    let mut info = mouser_platform::Platform::info(&platform);
    info.os = HostOs::Windows;
    platform.set_info(info);
    platform.set_secure_attention_supported(true);
    let f = fixture_on(platform.clone(), Arc::new(platform)).await;
    send(&f.host.control()).await.unwrap();
}

#[test]
fn a_refusal_is_shown_to_the_owner_and_success_is_silent() {
    let notifier = FakeNotifier::default();
    report(Ok(()), &notifier);
    assert!(notifier.shown.lock().unwrap().is_empty());
    report(
        Err("Ctrl-Alt-Del cannot be sent to Desk Mac.".into()),
        &notifier,
    );
    assert_eq!(
        *notifier.shown.lock().unwrap(),
        vec![(
            TITLE.to_string(),
            "Ctrl-Alt-Del cannot be sent to Desk Mac.".to_string()
        )]
    );
    let _: &dyn Notifier = &notifier;
}
