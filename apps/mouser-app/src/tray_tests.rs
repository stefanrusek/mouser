use mouser_proto::control::{ConnectionState, EngineState, Status};
use tauri::Manager;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

use crate::support_tests::{FakeTray, app_with, eventually, fixture, has_window, status_of};
use crate::tray::{
    ARRANGE_ID, Action, Appearance, HEALTH_ID, Icon, OPEN_ID, PAUSE_ID, QUIT_ID, RESUME_ID,
    TrayConnection, TrayController, UPDATE_ID, Variant, action, appearance_of, current_status,
    handle_menu, handle_tray_event, open, run_pause, system_appearance, view,
};

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

#[test]
fn the_variant_follows_the_appearance_so_a_coloured_dot_shows_on_macos_too() {
    let light = Icon::choose(Appearance::Light, false, TrayConnection::Alone);
    let dark = Icon::choose(Appearance::Dark, false, TrayConnection::Alone);
    assert_eq!(light.variant, Variant::Light);
    assert_eq!(dark.variant, Variant::Dark);
}

#[test]
fn all_sixteen_icons_are_distinct_pngs() {
    let mut seen = Vec::new();
    for variant in [Variant::Light, Variant::Dark] {
        for paused in [false, true] {
            for connection in [
                TrayConnection::Connected,
                TrayConnection::Partial,
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

/// `status` with exactly these desk-mates (this computer stays first).
fn with_desk_mates(mut status: Status, mates: &[(&str, ConnectionState)]) -> Status {
    status.computers.truncate(1);
    for (name, connection) in mates {
        let mut mate = status.computers[0].clone();
        mate.name = (*name).into();
        mate.connection = *connection;
        status.computers.push(mate);
    }
    status
}

#[tokio::test(flavor = "multi_thread")]
async fn the_dot_is_green_yellow_or_red_by_how_many_desk_mates_are_connected() {
    use ConnectionState::{Connected, Incompatible, Offline, Reconnecting};
    let f = fixture().await;
    let base = status_of(&f.host).await;
    let dot = |mates: &[(&str, ConnectionState)]| {
        let status = with_desk_mates(base.clone(), mates);
        (
            TrayConnection::of(&status),
            view(&status, Appearance::Dark, None),
        )
    };
    assert_eq!(dot(&[]).0, TrayConnection::Alone);
    assert_eq!(dot(&[("B", Connected)]).0, TrayConnection::Connected);
    assert_eq!(
        dot(&[("B", Connected), ("C", Connected)]).0,
        TrayConnection::Connected
    );
    assert_eq!(
        dot(&[("B", Connected), ("C", Offline)]).0,
        TrayConnection::Partial
    );
    assert_eq!(
        dot(&[("B", Reconnecting), ("C", Connected), ("D", Offline)]).0,
        TrayConnection::Partial
    );
    assert_eq!(dot(&[("B", Offline)]).0, TrayConnection::Disconnected);
    assert_eq!(
        dot(&[("B", Reconnecting), ("C", Incompatible)]).0,
        TrayConnection::Disconnected
    );
    // The icon carries the dot.
    let (_, partial) = dot(&[("B", Connected), ("C", Offline)]);
    assert_eq!(partial.icon.connection, TrayConnection::Partial);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tooltip_names_the_computers_that_are_not_connected_when_some_or_none_are() {
    use ConnectionState::{Connected, Offline, Reconnecting};
    let f = fixture().await;
    let base = status_of(&f.host).await;
    let tip = |mates: &[(&str, ConnectionState)]| {
        view(
            &with_desk_mates(base.clone(), mates),
            Appearance::Light,
            None,
        )
        .tooltip
    };
    assert_eq!(
        tip(&[]),
        "Mouser is active · no other computers on this desk"
    );
    assert_eq!(tip(&[("B", Connected)]), "Mouser is active · connected");
    assert_eq!(
        tip(&[("B", Connected), ("C", Offline), ("D", Reconnecting)]),
        "Mouser is active · not connected to C, D"
    );
    assert_eq!(
        tip(&[("B", Offline), ("C", Reconnecting)]),
        "Mouser is active · not connected to B, C"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_icon_follows_state_connection_and_theme_and_the_tooltip_names_both_states() {
    let f = fixture().await;
    let mut status = status_of(&f.host).await;
    let mut beta = status.computers[0].clone();
    beta.name = "Beta".into();
    beta.connection = ConnectionState::Offline;
    status.computers.push(beta);

    let alone_status = with_desk_mates(status.clone(), &[]);
    let alone = view(&alone_status, Appearance::Dark, None);
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

    let connected_status = with_desk_mates(status.clone(), &[("Beta", ConnectionState::Connected)]);
    let connected = view(&connected_status, Appearance::Light, None);
    assert_eq!(connected.icon.variant, Variant::Light);
    assert_eq!(connected.icon.connection, TrayConnection::Connected);
    assert_eq!(connected.tooltip, "Mouser is active · connected");

    status.state = EngineState::Paused;
    status.tray_summary.state = EngineState::Paused;
    let paused = view(&status, Appearance::Light, None);
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

    let active = view(&status, Appearance::Dark, None);
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
    let paused = view(&status, Appearance::Dark, None);
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
    let controller = TrayController::new(Box::new(port.clone()), Appearance::Light);
    controller.update_status(status.clone());
    controller.update_appearance(Appearance::Dark);
    let views = port.views.lock().unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0], view(&status, Appearance::Light, None));
    assert_eq!(views[1], view(&status, Appearance::Dark, None));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_status_twice_applies_once_and_a_changed_one_applies_again() {
    let f = fixture().await;
    let status = status_of(&f.host).await;
    let port = FakeTray::default();
    let controller = TrayController::new(Box::new(port.clone()), Appearance::Light);
    controller.update_status(status.clone());
    controller.update_status(status.clone());
    assert_eq!(port.views.lock().unwrap().len(), 1);
    let changed = with_desk_mates(status, &[("Beta", ConnectionState::Connected)]);
    controller.update_status(changed.clone());
    controller.update_status(changed);
    assert_eq!(port.views.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "a status arrived before any theme change")]
async fn a_theme_change_before_any_status_is_a_bug() {
    let port = FakeTray::default();
    TrayController::new(Box::new(port), Appearance::Light).update_appearance(Appearance::Dark);
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

async fn eventually_paused(control: &crate::host::Control, paused: bool) {
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

#[tokio::test(flavor = "multi_thread")]
async fn while_the_engine_is_away_the_tray_says_mouser_isnt_ready_and_comes_back_after() {
    let f = fixture().await;
    let status = status_of(&f.host).await;
    let port = FakeTray::default();
    let controller = TrayController::new(Box::new(port.clone()), Appearance::Light);
    let not_ready = |appearance| crate::tray_not_ready::view(appearance);
    // UI mode before the engine is up; a theme change then is no bug.
    controller.update_ready(false);
    controller.update_appearance(Appearance::Dark);
    // Ready again with no status yet: the next status draws the tray.
    controller.update_ready(true);
    controller.update_status(status.clone());
    // The engine goes and comes back: the last status again.
    controller.update_ready(false);
    controller.update_ready(true);
    let views = port.views.lock().unwrap();
    assert_eq!(
        *views,
        [
            not_ready(Appearance::Light),
            not_ready(Appearance::Dark),
            view(&status, Appearance::Dark, None),
            not_ready(Appearance::Dark),
            view(&status, Appearance::Dark, None),
        ]
    );
}
