use mouser_platform::Os;
use mouser_proto::control::{EngineState, TrayConnection};
use tauri::Manager;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

use crate::support_tests::{FakeTray, app_with, eventually, fixture, has_window, status_of};
use crate::tray::{
    ARRANGE_ID, Action, Appearance, HEALTH_ID, Icon, OPEN_ID, PAUSE_ID, QUIT_ID, RESUME_ID,
    TrayController, UPDATE_ID, Variant, action, appearance_of, current_status, handle_menu,
    handle_tray_event, open, run_pause, system_appearance, view,
};

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

#[test]
fn macos_always_uses_the_template_image() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let icon = Icon::choose(Os::MacOs, appearance, false, TrayConnection::Alone);
        assert_eq!(icon.variant, Variant::Template);
        assert!(icon.is_template());
    }
}

#[test]
fn other_systems_follow_the_system_appearance() {
    for os in [Os::Windows, Os::Linux] {
        let light = Icon::choose(os, Appearance::Light, false, TrayConnection::Alone);
        let dark = Icon::choose(os, Appearance::Dark, false, TrayConnection::Alone);
        assert_eq!(light.variant, Variant::Light);
        assert_eq!(dark.variant, Variant::Dark);
        assert!(!light.is_template() && !dark.is_template());
    }
}

#[test]
fn all_eighteen_icons_are_distinct_pngs() {
    let mut seen = Vec::new();
    for variant in [Variant::Template, Variant::Light, Variant::Dark] {
        for paused in [false, true] {
            for connection in [
                TrayConnection::Connected,
                TrayConnection::Disconnected,
                TrayConnection::Alone,
            ] {
                let png = Icon {
                    variant,
                    paused,
                    connection,
                }
                .png();
                assert!(is_png(png));
                assert!(!seen.contains(&png));
                seen.push(png);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_icon_follows_state_connection_and_theme_and_the_tooltip_names_both_states() {
    let f = fixture().await;
    let mut status = status_of(&f.host).await;
    let mut beta = status.computers[0].clone();
    beta.name = "Beta".into();
    beta.connection = mouser_proto::control::ConnectionState::Offline;
    status.computers.push(beta);

    status.tray_summary.connection = TrayConnection::Alone;
    let alone = view(&status, Os::Windows, Appearance::Dark, None);
    assert_eq!(
        alone.icon,
        Icon {
            variant: Variant::Dark,
            paused: false,
            connection: TrayConnection::Alone
        }
    );
    assert_eq!(
        alone.tooltip,
        "Mouser is active · no other computers on this desk"
    );

    status.tray_summary.connection = TrayConnection::Connected;
    let connected = view(&status, Os::MacOs, Appearance::Light, None);
    assert_eq!(connected.icon.variant, Variant::Template);
    assert_eq!(connected.icon.connection, TrayConnection::Connected);
    assert_eq!(connected.tooltip, "Mouser is active · connected");

    status.state = EngineState::Paused;
    status.tray_summary.state = EngineState::Paused;
    status.tray_summary.connection = TrayConnection::Disconnected;
    let paused = view(&status, Os::Linux, Appearance::Light, None);
    assert!(paused.icon.paused);
    assert_eq!(paused.icon.connection, TrayConnection::Disconnected);
    assert_eq!(paused.tooltip, "Mouser is paused · not connected to Beta");
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
    status.tray_summary.computers = 2;
    status.tray_summary.displays = 5;

    let active = view(&status, Os::MacOs, Appearance::Dark, None);
    assert_eq!(active.summary, "Mouser is active: 2 computers, 5 displays");
    assert!(!active.icon.paused);
    assert_eq!(active.entries[0].label, active.summary);
    assert_eq!(active.entries[0].id, None);
    assert_eq!(active.entries[1].id, Some(OPEN_ID));
    assert_eq!(active.entries[1].label, "Open Mouser");
    assert_eq!(active.entries[2].id, Some(PAUSE_ID));
    assert_eq!(active.entries[2].label, "Pause Mouser");
    assert_eq!(active.entries[3].id, Some(ARRANGE_ID));
    assert_eq!(active.entries[3].label, "Arrange displays");
    assert_eq!(active.entries[4].id, Some(HEALTH_ID));
    assert_eq!(active.entries[4].label, "Mouser health");
    assert_eq!(active.entries[5].id, Some(QUIT_ID));
    assert_eq!(active.entries[5].label, "Quit Mouser");

    status.tray_summary.state = EngineState::Paused;
    status.tray_summary.computers = 1;
    status.tray_summary.displays = 1;
    let paused = view(&status, Os::Windows, Appearance::Dark, None);
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
    assert_eq!(views[0], view(&status, Os::Linux, Appearance::Light, None));
    assert_eq!(views[1], view(&status, Os::Linux, Appearance::Dark, None));
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
    assert_eq!(action(OPEN_ID), Some(Action::Open));
    assert_eq!(action(ARRANGE_ID), Some(Action::Arrange));
    assert_eq!(action(HEALTH_ID), Some(Action::Health));
    assert_eq!(action(QUIT_ID), Some(Action::Quit));
    assert_eq!(action(PAUSE_ID), Some(Action::Pause));
    assert_eq!(action(RESUME_ID), Some(Action::Resume));
}

#[test]
fn an_unknown_menu_line_is_ignored() {
    assert_eq!(action("bogus"), None);
}

fn opened_at(app: &tauri::AppHandle<tauri::test::MockRuntime>) -> String {
    let window = app.get_webview_window("main").expect("main exists");
    window
        .url()
        .unwrap()
        .fragment()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn each_navigating_line_opens_its_section_in_the_one_window() {
    for (id, route) in [
        (OPEN_ID, "welcome"),
        (ARRANGE_ID, "displays"),
        (HEALTH_ID, "health"),
    ] {
        let f = fixture().await;
        let app = app_with(f.host);
        let handle = app.handle().clone();
        let control = app.state::<crate::host::Host>().control();
        let mut rest = Vec::new();
        handle_menu(&handle, &control, id, &mut |a| rest.push(a));
        eventually(|| has_window(&handle, "main")).await;
        assert_eq!(opened_at(&handle), route);
        assert!(rest.is_empty());
        assert_eq!(handle.webview_windows().len(), 1);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_and_resume_lines_act_on_the_engine_and_the_others_are_handed_on() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let host = app.state::<crate::host::Host>();
    let control = host.control();
    handle_menu(&handle, &control, PAUSE_ID, &mut |_| panic!("handled here"));
    eventually_paused(&control, true).await;
    handle_menu(&handle, &control, RESUME_ID, &mut |_| {
        panic!("handled here")
    });
    eventually_paused(&control, false).await;
    handle_menu(&handle, &control, "bogus", &mut |_| panic!("ignored"));
    let mut rest = Vec::new();
    for id in [UPDATE_ID, crate::tray_sas::SEND_CAD_ID, QUIT_ID] {
        handle_menu(&handle, &control, id, &mut |a| rest.push(a));
    }
    assert_eq!(rest, vec![Action::Update, Action::SendCad, Action::Quit]);
    assert!(!has_window(&handle, "main"));
}

async fn eventually_paused(control: &mouser_core::ControlHandle, paused: bool) {
    for _ in 0..200 {
        if (current_status(control).await.state == EngineState::Paused) == paused {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the engine did not reach paused={paused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_open_line_opens_mouser() {
    let f = fixture().await;
    let app = app_with(f.host);
    let handle = app.handle().clone();
    let control = app.state::<crate::host::Host>().control();
    open(&handle, &control);
    eventually(|| has_window(&handle, "main")).await;
    assert_eq!(opened_at(&handle), "welcome");
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

fn click(button: MouseButton, button_state: MouseButtonState) -> TrayIconEvent {
    TrayIconEvent::Click {
        id: "mouser".into(),
        position: tauri::PhysicalPosition::new(0.0, 0.0),
        rect: tauri::Rect {
            position: tauri::PhysicalPosition::new(0, 0).into(),
            size: tauri::PhysicalSize::new(16, 16).into(),
        },
        button,
        button_state,
    }
}

// Needs probe P4 (T015): left click reports `Click { Left, Up }` once per click.
#[tokio::test(flavor = "multi_thread")]
async fn a_left_click_up_toggles_pause_and_nothing_else_does() {
    let f = fixture().await;
    let control = f.host.control();
    // Down, right clicks and other events leave the engine alone.
    handle_tray_event(&control, &click(MouseButton::Left, MouseButtonState::Down));
    handle_tray_event(&control, &click(MouseButton::Right, MouseButtonState::Up));
    handle_tray_event(
        &control,
        &TrayIconEvent::Leave {
            id: "mouser".into(),
            position: tauri::PhysicalPosition::new(0.0, 0.0),
            rect: tauri::Rect {
                position: tauri::PhysicalPosition::new(0, 0).into(),
                size: tauri::PhysicalSize::new(16, 16).into(),
            },
        },
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(status_of(&f.host).await.state, EngineState::Active);

    handle_tray_event(&control, &click(MouseButton::Left, MouseButtonState::Up));
    eventually_paused(&control, true).await;
    handle_tray_event(&control, &click(MouseButton::Left, MouseButtonState::Up));
    eventually_paused(&control, false).await;
}
