use mouser_platform::Os;
use mouser_proto::control::EngineState;
use tauri::Manager;

use crate::support_tests::{FakeTray, app_with, eventually, fixture, has_window, status_of};
use crate::tray::{
    Action, Appearance, Icon, OPEN_ID, PAUSE_ID, QUIT_ID, RESUME_ID, TrayController, Variant,
    action, appearance_of, current_status, open, run_pause, system_appearance, view,
};

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

#[test]
fn macos_always_uses_the_template_image() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let icon = Icon::choose(Os::MacOs, appearance, false);
        assert_eq!(icon.variant, Variant::Template);
        assert!(icon.is_template());
    }
}

#[test]
fn other_systems_follow_the_system_appearance() {
    for os in [Os::Windows, Os::Linux] {
        let light = Icon::choose(os, Appearance::Light, false);
        let dark = Icon::choose(os, Appearance::Dark, false);
        assert_eq!(light.variant, Variant::Light);
        assert_eq!(dark.variant, Variant::Dark);
        assert!(!light.is_template() && !dark.is_template());
    }
}

#[test]
fn all_six_icons_are_distinct_pngs() {
    let mut seen = Vec::new();
    for variant in [Variant::Template, Variant::Light, Variant::Dark] {
        for paused in [false, true] {
            let png = Icon { variant, paused }.png();
            assert!(is_png(png));
            assert!(!seen.contains(&png));
            seen.push(png);
        }
    }
}

#[test]
fn the_theme_maps_to_an_appearance() {
    assert_eq!(appearance_of(tauri::Theme::Dark), Appearance::Dark);
    assert_eq!(appearance_of(tauri::Theme::Light), Appearance::Light);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tray_shows_active_or_paused_and_the_counts() {
    let f = fixture().await;
    let mut status = status_of(&f.host).await;
    let one = status.computers[0].clone();
    status.computers[0].displays = 3;
    status.computers.push(one);
    status.computers[1].displays = 2;

    let active = view(&status, Os::MacOs, Appearance::Dark);
    assert_eq!(active.summary, "Mouser is active: 2 computers, 5 displays");
    assert!(!active.icon.paused);
    assert_eq!(active.entries[0].label, active.summary);
    assert_eq!(active.entries[0].id, None);
    assert_eq!(active.entries[1].id, Some(OPEN_ID));
    assert_eq!(active.entries[1].label, "Open Mouser");
    assert_eq!(active.entries[2].id, Some(PAUSE_ID));
    assert_eq!(active.entries[2].label, "Pause Mouser");
    assert_eq!(active.entries[3].id, Some(QUIT_ID));
    assert_eq!(active.entries[3].label, "Quit Mouser");

    status.state = EngineState::Paused;
    status.computers.pop();
    status.computers[0].displays = 1;
    let paused = view(&status, Os::Windows, Appearance::Dark);
    assert_eq!(paused.summary, "Mouser is paused: 1 computer, 1 display");
    assert!(paused.icon.paused);
    assert_eq!(paused.icon.variant, Variant::Dark);
    assert_eq!(paused.entries[2].id, Some(RESUME_ID));
    assert_eq!(paused.entries[2].label, "Resume Mouser");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_controller_redraws_on_new_status_and_on_a_theme_change() {
    let f = fixture().await;
    let status = status_of(&f.host).await;
    let port = FakeTray::default();
    let controller = TrayController::new(Box::new(port.clone()), Os::Linux, Appearance::Light);
    controller.update_status(status.clone());
    controller.update_appearance(Appearance::Dark);
    let views = port.views.lock().unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0], view(&status, Os::Linux, Appearance::Light));
    assert_eq!(views[1], view(&status, Os::Linux, Appearance::Dark));
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "a status arrived before any theme change")]
async fn a_theme_change_before_any_status_is_a_bug() {
    let port = FakeTray::default();
    TrayController::new(Box::new(port), Os::Linux, Appearance::Light)
        .update_appearance(Appearance::Dark);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_system_appearance_comes_from_the_main_window() {
    let f = fixture().await;
    let app = app_with(f.host);
    tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(system_appearance(app.handle()), Appearance::Light);
    let status = current_status(&app.state::<crate::host::Host>().control()).await;
    assert_eq!(status.computer.name, "Desk Mac");
}

#[test]
fn menu_lines_map_to_their_actions() {
    assert_eq!(action(OPEN_ID), Action::Open);
    assert_eq!(action(QUIT_ID), Action::Quit);
    assert_eq!(action(PAUSE_ID), Action::Pause);
    assert_eq!(action(RESUME_ID), Action::Resume);
}

#[test]
#[should_panic(expected = "the tray has no menu line \"bogus\"")]
fn an_unknown_menu_line_is_a_bug() {
    action("bogus");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_open_line_opens_mouser() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let control = app.state::<crate::host::Host>().control();
    open(&handle, &control);
    eventually(|| has_window(&handle, "main")).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_pause_and_resume_lines_pause_and_resume_the_engine() {
    let f = fixture().await;
    let control = f.host.control();
    run_pause(&control, Action::Pause).await;
    assert_eq!(status_of(&f.host).await.state, EngineState::Paused);
    run_pause(&control, Action::Resume).await;
    assert_eq!(status_of(&f.host).await.state, EngineState::Active);
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "only Pause and Resume change the engine state")]
async fn other_lines_do_not_pause() {
    let f = fixture().await;
    run_pause(&f.host.control(), Action::Open).await;
}
