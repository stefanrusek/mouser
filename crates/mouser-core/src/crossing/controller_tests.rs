use mouser_platform::LocalInput;
use mouser_proto::control::Method;
use mouser_proto::model::Os;
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;

use crate::EngineEvent;
use crate::testing::*;

const A: u32 = 0x0007_0004;
const SHIFT: u32 = 0x0007_00E1;

fn key(usage: u32, down: bool) -> LocalInput {
    LocalInput::Key { usage, down }
}

fn motion(dx: f64) -> LocalInput {
    LocalInput::Motion {
        dx,
        dy: 0.0,
        abs: (960.0, 540.0),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[tokio::test]
async fn pushing_past_a_shared_edge_enters_the_adjoining_display() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut landing = b.engine.events();
    push_right(a);
    injected_where(
        &b.platform,
        "the pointer on Beta",
        |i| matches!(i, Injected::MotionAbs { x, y } if near(*x, 0.0) && near(*y, 540.0)),
    )
    .await;
    assert!(a.platform.is_grabbed(), "Alpha's own input is held back");
    assert!(matches!(
        landing.recv().await.unwrap(),
        EngineEvent::Landing { .. }
    ));
    // `status` says where the input goes, on both ends.
    eventually("the active computer to be Beta", || async {
        status(a).await.active_computer == Some(b.engine.computer_id())
            && status(b).await.active_computer == Some(b.engine.computer_id())
    })
    .await;
}

#[tokio::test]
async fn while_remote_motion_keys_buttons_and_scroll_act_on_the_active_computer() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    b.platform.take_injected();
    a.platform.send_input(motion(100.0));
    a.platform.send_input(key(A, true));
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    a.platform
        .send_input(LocalInput::Scroll { dx: 0.0, dy: -3.0 });
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: false,
    });
    a.platform.send_input(key(A, false));
    injected_where(&b.platform, "the key release", |i| {
        matches!(
            i,
            Injected::Key {
                usage: A,
                down: false
            }
        )
    })
    .await;
    let seen = b.platform.injected();
    let Injected::MotionAbs { x, y } = seen[0] else {
        panic!("motion first: {seen:?}");
    };
    assert!(near(x, 100.0) && near(y, 540.0), "{x} {y}");
    assert_eq!(
        seen[1..],
        [
            Injected::Key {
                usage: A,
                down: true
            },
            Injected::Button {
                button: 1,
                down: true
            },
            Injected::Scroll { dx: 0.0, dy: -3.0 },
            Injected::Button {
                button: 1,
                down: false
            },
            Injected::Key {
                usage: A,
                down: false
            },
        ]
    );
    // Nothing reached Alpha's own applications.
    assert!(
        a.platform
            .local_delivered()
            .iter()
            .all(|i| matches!(i, LocalInput::Motion { dx, .. } if *dx == 1.0)),
        "only the push that crossed was local"
    );
}

#[tokio::test]
async fn moving_back_returns_control_and_releases_what_the_remote_holds() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(key(A, true));
    injected_where(&b.platform, "the key", |i| {
        matches!(i, Injected::Key { down: true, .. })
    })
    .await;
    // 600 counts left of Beta's left edge is 1320 px into Alpha.
    a.platform.send_input(motion(-600.0));
    eventually("control to come back", || async {
        !a.platform.is_grabbed()
    })
    .await;
    injected_where(&b.platform, "the release", |i| {
        matches!(
            i,
            Injected::Key {
                usage: A,
                down: false
            }
        )
    })
    .await;
    let warps = a.platform.warps();
    assert!(
        near(warps[0].0, 1320.0) && near(warps[0].1, 540.0),
        "{warps:?}"
    );
    // Input is local again.
    a.platform.send_input(key(A, true));
    assert_eq!(a.platform.local_delivered().last(), Some(&key(A, true)));
    eventually("no active computer", || async {
        status(a).await.active_computer.is_none()
    })
    .await;
}

#[tokio::test]
async fn any_computer_can_be_the_controller() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // The devices are plugged into Beta this time, and it pushes left.
    push_left(b);
    injected_where(
        &a.platform,
        "the pointer on Alpha",
        |i| matches!(i, Injected::MotionAbs { x, .. } if near(*x, 1919.0 - 0.0) || *x > 1900.0),
    )
    .await;
    assert!(b.platform.is_grabbed());
    b.platform.send_input(key(A, true));
    injected_where(&a.platform, "the key on Alpha", |i| {
        matches!(
            i,
            Injected::Key {
                usage: A,
                down: true
            }
        )
    })
    .await;
}

#[tokio::test]
async fn different_densities_keep_the_position_and_the_physical_speed() {
    let lab = Lab::new();
    // Beta's display is 1280x720 points, same 527x296 mm: twice the density.
    let beta = VirtualPlatform::new();
    let mut screen = screen("big", (0.0, 0.0));
    screen.logical_size = (1280.0, 720.0);
    screen.pixel_size = (2560, 1440);
    screen.scale = 2.0;
    beta.set_displays(vec![screen]);
    let a = lab.start("Alpha").await;
    let b = lab.start_with("Beta", beta, |_| {}).await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    // Half way down Alpha is half way down Beta.
    injected_where(
        &b.platform,
        "entry",
        |i| matches!(i, Injected::MotionAbs { x, y } if near(*x, 0.0) && near(*y, 360.0)),
    )
    .await;
    // 100 counts are 27.45 mm, which is 66.67 of Beta's coarser pixels.
    a.platform.send_input(motion(100.0));
    injected_where(&b.platform, "motion", |i| {
        matches!(i, Injected::MotionAbs { x, .. } if (*x - 100.0 * 1280.0 / 1920.0).abs() < 1e-3)
    })
    .await;
}

#[tokio::test]
async fn every_display_of_a_computer_is_reachable() {
    let lab = Lab::new();
    // Alpha has two displays side by side; Beta sits right of both.
    let alpha = VirtualPlatform::new();
    alpha.set_displays(vec![
        screen("left", (0.0, 0.0)),
        screen("right", (1920.0, 0.0)),
    ]);
    let a = lab.start_with("Alpha", alpha, |_| {}).await;
    let b = lab.start("Beta").await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    for node in &nodes {
        eventually("three displays placed", || async {
            node.engine.ctx.crossing.lock().unwrap().map.placed.len() == 3
        })
        .await;
    }
    let (a, b) = (&nodes[0], &nodes[1]);
    // Beta's left edge leads to Alpha's right display, then its left display.
    push_left(b);
    injected_where(
        &a.platform,
        "Alpha's right display",
        |i| matches!(i, Injected::MotionAbs { x, .. } if *x >= 1920.0),
    )
    .await;
    a.platform.take_injected();
    b.platform.send_input(motion(-5000.0));
    injected_where(
        &a.platform,
        "Alpha's left display",
        |i| matches!(i, Injected::MotionAbs { x, .. } if *x < 1920.0),
    )
    .await;
}

#[tokio::test]
async fn control_passes_between_remote_computers() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let (a, b, c) = (&nodes[0], &nodes[1], &nodes[2]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(key(A, true));
    injected_where(&b.platform, "key", |i| {
        matches!(i, Injected::Key { down: true, .. })
    })
    .await;
    // Right across Beta and onto Gamma.
    a.platform.send_input(motion(1920.0));
    injected_where(&c.platform, "Gamma", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    injected_where(&b.platform, "Beta released", |i| {
        matches!(
            i,
            Injected::Key {
                usage: A,
                down: false
            }
        )
    })
    .await;
    assert!(a.platform.is_grabbed());
    assert_eq!(
        status(a).await.active_computer,
        Some(c.engine.computer_id())
    );
}

#[tokio::test]
async fn a_paused_computer_starts_no_crossing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let _: mouser_proto::control::Empty = call(a, Method::ActivePause, json!({})).await;
    push_right(a);
    settle().await;
    assert!(!a.platform.is_grabbed());
    assert!(b.platform.injected().is_empty());
}

#[tokio::test]
async fn pushing_where_nothing_adjoins_or_off_the_displays_does_nothing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    // Alpha's left edge has no neighbour; neither has its top.
    push_left(a);
    a.platform.send_input(LocalInput::Motion {
        dx: 0.0,
        dy: -1.0,
        abs: (10.0, 0.0),
    });
    // A pointer position on no display, and a move that stays on the display.
    a.platform.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (-500.0, 0.0),
    });
    a.platform.send_input(motion(10.0));
    settle().await;
    assert!(!a.platform.is_grabbed());
    assert!(nodes[1].platform.injected().is_empty());
}

#[tokio::test]
async fn held_keys_and_buttons_are_released_where_they_are_left() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform.send_input(key(SHIFT, true));
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    // Alpha's applications are told the keys went up, since the grab will
    // swallow the physical release.
    let local = a.platform.injected();
    assert!(
        local.contains(&Injected::Key {
            usage: SHIFT,
            down: false
        }),
        "{local:?}"
    );
    assert!(
        local.contains(&Injected::Button {
            button: 1,
            down: false
        }),
        "{local:?}"
    );
    // The physical releases are not forwarded to Beta, which never saw the presses.
    a.platform.send_input(key(SHIFT, false));
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: false,
    });
    settle().await;
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Key { .. } | Injected::Button { .. }))
    );
}

#[tokio::test]
async fn losing_the_active_computer_returns_control() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let b = nodes.pop().unwrap();
    let a = &nodes[0];
    push_right(a);
    eventually("the grab", || async { a.platform.is_grabbed() }).await;
    b.engine.shutdown().await;
    eventually("control to return", || async { !a.platform.is_grabbed() }).await;
    assert_eq!(a.platform.warps().len(), 1);
}

#[tokio::test]
async fn an_os_pair_is_known_from_hello() {
    let lab = Lab::new();
    let mac = VirtualPlatform::new();
    mac.set_info(mouser_platform::PlatformInfo {
        os: mouser_platform::Os::MacOs,
        version: "26".into(),
        session: mouser_platform::SessionType::Native,
        limitations: Vec::new(),
    });
    let a = lab.start_with("Mac", mac, |_| {}).await;
    let b = lab.start("Linux").await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let wires = &b.engine.ctx.crossing.lock().unwrap().map.wires;
    assert_eq!(wires[&a.engine.computer_id()].os, Os::Macos);
}

#[tokio::test]
async fn the_remote_pointer_stops_at_the_edge_of_the_last_display() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(motion(100000.0));
    injected_where(
        &b.platform,
        "the wall",
        |i| matches!(i, Injected::MotionAbs { x, .. } if near(*x, 1919.0)),
    )
    .await;
    assert!(a.platform.is_grabbed(), "the pointer did not leave");
}

fn mac() -> VirtualPlatform {
    let platform = VirtualPlatform::new();
    platform.set_info(mouser_platform::PlatformInfo {
        os: mouser_platform::Os::MacOs,
        version: "26".into(),
        session: mouser_platform::SessionType::Native,
        limitations: Vec::new(),
    });
    platform
}

/// The owner's report on v0.2.1: Command-C and Command-V typed on one Mac's
/// keyboard while the pointer is on the other Mac. The copy there is shared
/// back while Command is still held; neither the clipboard traffic nor the
/// paste shortcut may hold back, reorder or drop the Command release.
#[tokio::test]
async fn command_c_and_v_on_another_mac_release_command_while_the_clipboard_is_shared() {
    const C: u32 = 0x0007_0006;
    const V: u32 = 0x0007_0019;
    const META: u32 = 0x0007_00E3;
    let lab = Lab::new();
    let a = lab.start_with("Mini", mac(), |_| {}).await;
    let b = lab.start_with("Book", mac(), |_| {}).await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    b.platform.take_injected();
    let text = |t: &str| vec![mouser_platform::ClipRep::Text(t.into())];

    a.platform.send_input(key(META, true));
    a.platform.send_input(key(C, true));
    injected_where(&b.platform, "C on the Book", |i| {
        *i == Injected::Key {
            usage: C,
            down: true,
        }
    })
    .await;
    // The application on the Book copies while Command is still down.
    b.platform.set_local_clipboard(text("copied"));
    eventually("the copy to reach the Mini", || async {
        a.platform.clipboard_contents() == text("copied")
    })
    .await;
    a.platform.send_input(key(C, false));
    a.platform.send_input(key(META, false));
    // Then a paste, which tells the Book's clipboard task.
    a.platform.send_input(key(META, true));
    a.platform.send_input(key(V, true));
    a.platform.send_input(key(V, false));
    a.platform.send_input(key(META, false));
    let releases = |platform: &VirtualPlatform| {
        platform
            .injected()
            .iter()
            .filter(|i| {
                **i == Injected::Key {
                    usage: META,
                    down: false,
                }
            })
            .count()
    };
    eventually("both Command releases on the Book", || async {
        releases(&b.platform) == 2
    })
    .await;
    let keys: Vec<Injected> = b
        .platform
        .injected()
        .into_iter()
        .filter(|i| matches!(i, Injected::Key { .. }))
        .collect();
    let k = |usage, down| Injected::Key { usage, down };
    assert_eq!(
        keys,
        [
            k(META, true),
            k(C, true),
            k(C, false),
            k(META, false),
            k(META, true),
            k(V, true),
            k(V, false),
            k(META, false),
        ]
    );
    // The Book's text was pasted on the Book, not replaced by a stale offer.
    assert_eq!(b.platform.clipboard_contents(), text("copied"));
    // And nothing typed reached the Mini's own applications.
    assert!(
        a.platform
            .local_delivered()
            .iter()
            .all(|i| !matches!(i, LocalInput::Key { .. })),
        "{:?}",
        a.platform.local_delivered()
    );
}
