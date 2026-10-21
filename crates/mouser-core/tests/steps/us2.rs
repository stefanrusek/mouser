//! Step definitions for `features/us2.feature`: one mouse and keyboard drive
//! every computer on the desk.

use std::fmt;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_core::EngineEvent;
use mouser_core::control::status::tray_summary;
use mouser_platform::{
    Capability, Limitation, LocalDisplay, LocalInput, Os, Platform, PlatformInfo,
    SecureInputHolder, SessionType,
};
use mouser_proto::control::{
    ConnectionState, Empty, EngineState, JoinPending, Method, PendingJoin, ProblemKind, Topic,
};
use mouser_proto::model::DisplayId;
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

const A: u32 = 0x0007_0004;
const C: u32 = 0x0007_0006;
const ESC: u32 = 0x0007_0029;
const CTRL: u32 = 0x0007_00E0;
const ALT: u32 = 0x0007_00E2;
const META: u32 = 0x0007_00E3;
const LEFT_ARROW: u32 = 0x0007_0050;
const RIGHT_ARROW: u32 = 0x0007_004F;

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct CrossWorld {
    pub lab: Lab,
    /// Displays on the desk, for knowing when the layout is complete.
    displays: usize,
    /// Review offers Alpha had seen before the step under test.
    reviews_seen: usize,
}

impl fmt::Debug for CrossWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CrossWorld({:?})", self.lab.names())
    }
}

fn os_info(os: Os, limitations: &[&str]) -> PlatformInfo {
    PlatformInfo {
        os,
        version: "test".into(),
        session: SessionType::Native,
        limitations: limitations
            .iter()
            .map(|text| Limitation {
                capability: Capability::Inject,
                text: (*text).into(),
            })
            .collect(),
    }
}

fn platform_of(os: Os) -> VirtualPlatform {
    let platform = VirtualPlatform::new();
    platform.set_info(os_info(os, &[]));
    platform
}

fn screen(key: &str, origin: (f64, f64)) -> LocalDisplay {
    LocalDisplay {
        key: key.into(),
        name: key.into(),
        pixel_size: (1920, 1080),
        logical_size: (1920.0, 1080.0),
        scale: 1.0,
        physical_size_mm: Some((527.0, 296.0)),
        origin,
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn key(usage: u32, down: bool) -> LocalInput {
    LocalInput::Key { usage, down }
}

fn button(button: u8, down: bool) -> LocalInput {
    LocalInput::Button { button, down }
}

fn motion(dx: f64) -> LocalInput {
    LocalInput::Motion {
        dx,
        dy: 0.0,
        abs: (960.0, 540.0),
    }
}

/// Push the shared mouse off the right edge of the computer's display.
fn push_right(platform: &VirtualPlatform) {
    platform.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (1919.0, 540.0),
    });
}

/// Push the shared mouse off the left edge of the computer's display.
fn push_left(platform: &VirtualPlatform) {
    platform.send_input(LocalInput::Motion {
        dx: -1.0,
        dy: 0.0,
        abs: (0.0, 540.0),
    });
}

/// Wait until `what` holds.
async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Wait until the platform was asked to inject something matching `pred`.
async fn injected(platform: &VirtualPlatform, what: &str, pred: impl Fn(&Injected) -> bool) {
    until(what, || platform.injected().iter().any(&pred)).await;
}

impl CrossWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            displays: 0,
            reviews_seen: 0,
        }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    /// `joiner` joins `member`'s desk by address and `member`'s owner allows it.
    async fn join(&mut self, member: &str, joiner: &str) {
        let target = self.lab.node(member).sim.clone();
        let address = self.lab.node(joiner).sim.route_to(&target).to_string();
        let _: JoinPending = self
            .lab
            .node(joiner)
            .call(Method::DeskJoin, json!({ "address": address }))
            .await;
        let value = self
            .lab
            .node(member)
            .notification("a join request", Topic::JoinRequest, |p| {
                p["computer"]["name"] == joiner
            })
            .await;
        let request: PendingJoin = serde_json::from_value(value).unwrap();
        let _: Empty = self
            .lab
            .node(member)
            .call(
                Method::JoinDecide,
                json!({"request_id": request.request_id, "approve": true}),
            )
            .await;
    }

    /// Wait until every computer sees the others connected, their displays,
    /// and the complete layout.
    async fn settled(&mut self) {
        let names = self.lab.names();
        let total = self.displays;
        for name in &names {
            let node = self.lab.node(name);
            node.wait_status("every computer connected with its displays", |s| {
                s.computers.len() == names.len()
                    && s.computers.iter().all(|c| {
                        c.connection == ConnectionState::SelfComputer
                            || (c.connection == ConnectionState::Connected && c.displays > 0)
                    })
            })
            .await;
            until("the whole layout", || {
                node.layout().placements.len() == total
            })
            .await;
        }
    }

    /// A desk of computers in this order, each with `setup`, the first creating it.
    async fn desk(&mut self, computers: Vec<(&str, Setup)>) {
        for (name, setup) in &computers {
            self.displays += setup.platform.displays().expect("virtual displays").len();
            self.lab.add_with(name, setup.clone()).await;
        }
        let first = computers[0].0;
        let _: mouser_proto::control::Desk = self
            .lab
            .node(first)
            .call(Method::DeskCreate, json!({"name": "Home office"}))
            .await;
        for (name, _) in &computers[1..] {
            self.join(first, name).await;
        }
        self.settled().await;
    }

    /// Default computers called `names`.
    async fn default_desk(&mut self, names: &[&str]) {
        let computers = names
            .iter()
            .map(|n| (*n, Setup::default()))
            .collect::<Vec<_>>();
        self.desk(computers).await;
    }
}

fn setup_of(platform: VirtualPlatform) -> Setup {
    Setup {
        platform,
        ..Setup::default()
    }
}

// ---- scenario 1: automatic placement -----------------------------------------

#[given("two computers are on a desk and a new computer joins")]
async fn two_on_a_desk(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    w.lab.add("Gamma").await;
    w.displays += 1;
}

#[when("the join completes")]
async fn the_join_completes(w: &mut CrossWorld) {
    w.join("Alpha", "Gamma").await;
    w.settled().await;
}

#[then(
    "its displays are placed next to the existing layout automatically and the desk owner is offered a review of the placement"
)]
async fn placed_and_review_offered(w: &mut CrossWorld) {
    let gamma = w.lab.node("Gamma").id();
    let offer = w
        .lab
        .node("Alpha")
        .engine_event(
            "a review offer for Gamma",
            0,
            |e| matches!(e, EngineEvent::ReviewOffered { computer, .. } if *computer == gamma),
        )
        .await;
    assert!(matches!(offer, EngineEvent::ReviewOffered { name, .. } if name == "Gamma"));
    // Alpha at the origin, Beta after it, Gamma after both, tops aligned.
    let mut at: Vec<_> = w
        .lab
        .node("Alpha")
        .layout()
        .placements
        .into_iter()
        .map(|p| (p.x, p.y, p.display_id.computer))
        .collect();
    at.sort();
    assert_eq!(
        at.iter().map(|(x, y, _)| (*x, *y)).collect::<Vec<_>>(),
        [(0, 0), (527, 0), (1054, 0)]
    );
    assert_eq!(at[2].2, gamma);
    assert!(
        w.lab
            .node("Alpha")
            .needs_review()
            .iter()
            .any(|d| d.computer == gamma)
    );
    for name in ["Beta", "Gamma"] {
        assert_eq!(
            w.lab.node(name).layout(),
            w.lab.node("Alpha").layout(),
            "{name} agrees on the layout"
        );
    }
}

// ---- scenario 2: crossing an edge ----------------------------------------------

#[given("the pointer is on a display whose edge adjoins another computer's display")]
async fn pointer_by_an_edge(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
}

#[when("the desk user moves it past that edge")]
async fn moves_past_the_edge(w: &mut CrossWorld) {
    let alpha = w.platform("Alpha");
    push_right(&alpha);
}

#[then(
    "it appears at the corresponding position on the adjoining display, and keystrokes, mouse buttons, and scrolling now act on that computer"
)]
async fn appears_and_input_follows(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    injected(
        &beta,
        "the pointer on Beta",
        |i| matches!(i, Injected::MotionAbs { x, y } if near(*x, 0.0) && near(*y, 540.0)),
    )
    .await;
    beta.take_injected();
    alpha.send_input(key(A, true));
    alpha.send_input(button(1, true));
    alpha.send_input(LocalInput::Scroll { dx: 0.0, dy: -3.0 });
    alpha.send_input(button(1, false));
    alpha.send_input(key(A, false));
    injected(&beta, "the typed input", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    assert_eq!(
        beta.injected(),
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
    assert!(
        alpha
            .local_delivered()
            .iter()
            .all(|i| matches!(i, LocalInput::Motion { .. })),
        "Alpha's own applications got none of it"
    );
}

// ---- scenario 3: any computer can be the controller ---------------------------------

#[given("any computer on the desk has the pointer")]
async fn any_computer_has_the_pointer(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta", "Gamma"]).await;
}

#[when("the desk user moves it to any other computer's display")]
async fn moves_to_other_computers(_w: &mut CrossWorld) {
    // The crossings are made and checked in the next step, one controller at a time.
}

#[then(
    "the crossing works the same way regardless of which computer the mouse and keyboard are physically plugged into"
)]
async fn works_from_every_computer(w: &mut CrossWorld) {
    // (devices on, edge pushed, computer entered, and how to come back)
    let legs: [(&str, bool, &str, f64); 4] = [
        ("Alpha", true, "Beta", -2000.0),
        ("Beta", true, "Gamma", -2000.0),
        ("Gamma", false, "Beta", 2000.0),
        ("Beta", false, "Alpha", 2000.0),
    ];
    for (controller, right, target, back) in legs {
        let (c, t) = (w.platform(controller), w.platform(target));
        t.take_injected();
        if right {
            push_right(&c);
        } else {
            push_left(&c);
        }
        injected(&t, &format!("{controller} to enter {target}"), |i| {
            matches!(i, Injected::MotionAbs { .. })
        })
        .await;
        assert!(c.is_grabbed(), "{controller} holds its devices");
        c.send_input(motion(back));
        until(&format!("{controller} to get control back"), || {
            !c.is_grabbed()
        })
        .await;
    }
}

// ---- scenario 4: shortcuts -----------------------------------------------------------

#[given("computers run different operating systems (macOS, Windows, Linux)")]
async fn different_operating_systems(w: &mut CrossWorld) {
    w.desk(vec![
        ("Mac", setup_of(platform_of(Os::MacOs))),
        ("Win", setup_of(platform_of(Os::Windows))),
        ("Tux", setup_of(platform_of(Os::Linux))),
    ])
    .await;
}

#[when("the desk user presses a shortcut on the active computer")]
async fn presses_a_shortcut(w: &mut CrossWorld) {
    // Command-C on the Mac keyboard, with Windows active.
    let (mac, win) = (w.platform("Mac"), w.platform("Win"));
    push_right(&mac);
    injected(&win, "Windows entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    win.take_injected();
    for (usage, down) in [(META, true), (C, true), (C, false), (META, false)] {
        mac.send_input(key(usage, down));
    }
    injected(&win, "the shortcut", |i| {
        *i == Injected::Key {
            usage: CTRL,
            down: false,
        }
    })
    .await;
}

#[then(
    "it has the effect that shortcut is expected to have on that computer's operating system (for example ⌘C on a Mac keyboard copies on Windows)"
)]
async fn shortcut_has_the_local_effect(w: &mut CrossWorld) {
    let (mac, win, tux) = (w.platform("Mac"), w.platform("Win"), w.platform("Tux"));
    assert_eq!(
        win.injected(),
        [
            Injected::Key {
                usage: CTRL,
                down: true
            },
            Injected::Key {
                usage: C,
                down: true
            },
            Injected::Key {
                usage: C,
                down: false
            },
            Injected::Key {
                usage: CTRL,
                down: false
            },
        ],
        "Command-C is Control-C on Windows"
    );
    // The other way: Control-C on the Windows keyboard is Command-C on the Mac.
    mac.send_input(motion(-2000.0));
    until("the Mac to get control back", || !mac.is_grabbed()).await;
    mac.take_injected();
    push_left(&win);
    injected(&mac, "the Mac entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    mac.take_injected();
    for (usage, down) in [(CTRL, true), (C, true), (C, false), (CTRL, false)] {
        win.send_input(key(usage, down));
    }
    injected(&mac, "the shortcut", |i| {
        *i == Injected::Key {
            usage: META,
            down: false,
        }
    })
    .await;
    assert_eq!(
        mac.injected(),
        [
            Injected::Key {
                usage: META,
                down: true
            },
            Injected::Key {
                usage: C,
                down: true
            },
            Injected::Key {
                usage: C,
                down: false
            },
            Injected::Key {
                usage: META,
                down: false
            },
        ],
        "Control-C is Command-C on the Mac"
    );
    // Windows and Linux share their shortcuts.
    win.send_input(motion(2000.0));
    until("Windows to get control back", || !win.is_grabbed()).await;
    push_right(&win);
    injected(&tux, "Linux entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    tux.take_injected();
    win.send_input(key(CTRL, true));
    injected(&tux, "Control", |i| {
        *i == Injected::Key {
            usage: CTRL,
            down: true,
        }
    })
    .await;
}

// ---- scenario 5: pixel densities --------------------------------------------------------

#[given("displays with different pixel densities or scaling")]
async fn different_densities(w: &mut CrossWorld) {
    let beta = VirtualPlatform::new();
    // Twice the density: 1280x720 points for the same 527x296 mm.
    let mut fine = screen("fine", (0.0, 0.0));
    fine.logical_size = (1280.0, 720.0);
    fine.pixel_size = (2560, 1440);
    fine.scale = 2.0;
    beta.set_displays(vec![fine]);
    w.desk(vec![("Alpha", Setup::default()), ("Beta", setup_of(beta))])
        .await;
}

#[when("the pointer crosses between them")]
async fn crosses_between_them(w: &mut CrossWorld) {
    let alpha = w.platform("Alpha");
    push_right(&alpha);
    let beta = w.platform("Beta");
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[then(
    "it moves at a consistent physical speed and lands at the matching position along the shared edge"
)]
async fn consistent_speed_and_position(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    // Half way down Alpha is half way down Beta.
    assert!(
        beta.injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { x, y } if near(*x, 0.0) && near(*y, 360.0))),
        "{:?}",
        beta.injected()
    );
    // 100 counts are 27.45 mm: 66.67 of Beta's coarser points.
    alpha.send_input(motion(100.0));
    injected(
        &beta,
        "motion",
        |i| matches!(i, Injected::MotionAbs { x, .. } if near(*x, 100.0 * 1280.0 / 1920.0)),
    )
    .await;
    // And back across the edge half way down Beta is half way down Alpha.
    alpha.send_input(LocalInput::Motion {
        dx: -100000.0,
        dy: 0.0,
        abs: (960.0, 540.0),
    });
    until("Alpha to get control back", || !alpha.is_grabbed()).await;
    let warps = alpha.warps();
    assert!(near(warps[0].1, 540.0), "{warps:?}");
}

// ---- scenario 6: every display reachable ---------------------------------------------------

#[given("a computer has several displays")]
async fn a_computer_with_several_displays(w: &mut CrossWorld) {
    let alpha = VirtualPlatform::new();
    alpha.set_displays(vec![
        screen("left", (0.0, 0.0)),
        screen("right", (1920.0, 0.0)),
    ]);
    w.desk(vec![("Alpha", setup_of(alpha)), ("Beta", Setup::default())])
        .await;
}

#[when("the pointer moves among them and to other computers")]
async fn moves_among_displays(w: &mut CrossWorld) {
    // From Beta's left edge onto Alpha's right display, then on to its left one.
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_left(&beta);
    injected(
        &alpha,
        "Alpha's right display",
        |i| matches!(i, Injected::MotionAbs { x, .. } if *x >= 1920.0),
    )
    .await;
    beta.send_input(motion(-5000.0));
    injected(
        &alpha,
        "Alpha's left display",
        |i| matches!(i, Injected::MotionAbs { x, .. } if *x < 1920.0),
    )
    .await;
    // Back to Beta over both displays, from Alpha's own devices.
    for _ in 0..3 {
        beta.send_input(motion(5000.0));
    }
    until("Beta to get control back", || !beta.is_grabbed()).await;
    // Alpha's pointer is on its right display, at the edge toward Beta.
    alpha.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (3839.0, 540.0),
    });
}

#[then("every display on the desk is reachable through the layout")]
async fn every_display_is_reachable(w: &mut CrossWorld) {
    let beta = w.platform("Beta");
    injected(&beta, "Beta's display", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    let layout = w.lab.node("Alpha").layout();
    assert_eq!(layout.placements.len(), 3, "all three displays are placed");
}

// ---- scenario 7 and the accepted limitation ---------------------------------------------------

const IGNORED: &str = "Applications that ignore simulated input are not reached on this computer.";

#[given(
    "an application on the active computer ignores ordinary simulated input (such as some games and remote-desktop clients)"
)]
async fn an_application_ignores_input(w: &mut CrossWorld) {
    let beta = VirtualPlatform::new();
    beta.set_info(os_info(Os::Windows, &[IGNORED]));
    w.desk(vec![("Alpha", Setup::default()), ("Beta", setup_of(beta))])
        .await;
}

#[when("the desk user types and clicks in it")]
async fn types_and_clicks(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    beta.take_injected();
    alpha.send_input(key(A, true));
    alpha.send_input(key(A, false));
    alpha.send_input(button(1, true));
    alpha.send_input(button(1, false));
}

#[then("the input reaches the application")]
async fn input_reaches_the_application(w: &mut CrossWorld) {
    // Mouser hands every key and click to the operating system's injection;
    // whether that application accepts it is the limitation stated elsewhere.
    let beta = w.platform("Beta");
    injected(&beta, "the click", |i| {
        *i == Injected::Button {
            button: 1,
            down: false,
        }
    })
    .await;
    assert_eq!(
        beta.injected(),
        [
            Injected::Key {
                usage: A,
                down: true
            },
            Injected::Key {
                usage: A,
                down: false
            },
            Injected::Button {
                button: 1,
                down: true
            },
            Injected::Button {
                button: 1,
                down: false
            },
        ]
    );
}

#[given("applications that ignore simulated input are not reached on macOS or Windows")]
async fn not_reached_on_macos_or_windows(w: &mut CrossWorld) {
    let windows = VirtualPlatform::new();
    windows.set_info(os_info(Os::Windows, &[IGNORED]));
    w.lab.add_with("Win", setup_of(windows)).await;
}

#[when("the desk user looks at the app on the affected computer")]
async fn looks_at_the_app(_w: &mut CrossWorld) {
    // The app renders `status`; the next step reads it.
}

#[then("the app states that applications ignoring simulated input are not reached")]
async fn app_states_the_limitation(w: &mut CrossWorld) {
    let status = w.lab.node("Win").status().await;
    assert!(
        status.computers[0].limitations.iter().any(|l| l == IGNORED),
        "{:?}",
        status.computers[0].limitations
    );
    assert!(
        status
            .problems
            .iter()
            .any(|p| { p.kind == ProblemKind::PlatformLimitation && p.message == IGNORED })
    );
}

// ---- scenario 8: panic key ---------------------------------------------------------------------

#[given("the pointer is on another computer")]
async fn pointer_on_another_computer(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    alpha.send_input(key(A, true));
    injected(&beta, "a key held on Beta", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
}

#[when("the desk user presses the panic key")]
async fn presses_the_panic_key(w: &mut CrossWorld) {
    let alpha = w.platform("Alpha");
    for _ in 0..2 {
        alpha.send_input(key(ESC, true));
        alpha.send_input(key(ESC, false));
    }
}

#[then("the pointer and keyboard return to the computer they are physically at")]
async fn control_returns_home(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    until("Alpha to get control back", || !alpha.is_grabbed()).await;
    injected(&beta, "Beta to release the held key", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    assert_eq!(
        alpha.warps(),
        [(1919.0, 540.0)],
        "the pointer is where it left"
    );
    assert!(
        alpha.send_input(key(A, true)),
        "the keyboard types here again"
    );
    let active = w.lab.node("Alpha").status().await.active_computer;
    assert_eq!(active, None);
}

// ---- scenario 9: landing highlight -----------------------------------------------------------------

#[given("the pointer crosses to another computer")]
async fn pointer_crosses(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    let alpha = w.platform("Alpha");
    push_right(&alpha);
}

#[when("it arrives")]
async fn it_arrives(w: &mut CrossWorld) {
    let beta = w.platform("Beta");
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[then("a brief highlight shows where it landed")]
async fn highlight_shows_where(w: &mut CrossWorld) {
    let beta_id = w.lab.node("Beta").id();
    let landing = w
        .lab
        .node("Beta")
        .engine_event("a landing", 0, |e| matches!(e, EngineEvent::Landing { .. }))
        .await;
    let EngineEvent::Landing {
        display,
        x,
        y,
        desktop,
    } = landing
    else {
        unreachable!("matched above");
    };
    assert_eq!(
        display,
        DisplayId {
            computer: beta_id,
            key: "virtual-0".into()
        }
    );
    assert!(near(f64::from(x), 0.0) && near(f64::from(y), 540.0));
    assert!(near(desktop.0, 0.0) && near(desktop.1, 540.0));
}

// ---- scenario 10: Secure Input -------------------------------------------------------------------------

#[given("a macOS computer has an application holding Secure Input")]
async fn mac_with_secure_input(w: &mut CrossWorld) {
    w.desk(vec![
        ("Mac", setup_of(platform_of(Os::MacOs))),
        ("Tux", Setup::default()),
    ])
    .await;
    w.platform("Mac").set_secure_input(Some(SecureInputHolder {
        pid: 42,
        app_name: "1Password".into(),
    }));
}

#[when("that blocks Mouser on that computer")]
async fn secure_input_blocks(w: &mut CrossWorld) {
    // Mouser notices on its own; wait for the Mac to say so.
    w.lab
        .node("Mac")
        .notification("the Secure Input notice", Topic::Problem, |p| {
            p["kind"] == json!(ProblemKind::SecureInput)
        })
        .await;
}

#[then("the desk user sees a notification naming the application")]
async fn notification_names_the_application(w: &mut CrossWorld) {
    for name in ["Mac", "Tux"] {
        let notice = w
            .lab
            .node(name)
            .notification("the Secure Input notice", Topic::Problem, |p| {
                p["kind"] == json!(ProblemKind::SecureInput)
            })
            .await;
        let message = notice["message"].as_str().unwrap();
        assert!(message.contains("1Password"), "{name}: {message}");
        assert!(message.contains("Mac"), "{name}: {message}");
    }
    // The connection stays up.
    let status = w.lab.node("Tux").status().await;
    assert_eq!(status.computers[1].connection, ConnectionState::Connected);
}

// ---- scenario 11: buttons 4 and 5 --------------------------------------------------------------------------

#[given(
    "the desk user has assigned mouse buttons 4 and 5 to pass through unchanged or to act as Back and Forward"
)]
async fn buttons_assigned(w: &mut CrossWorld) {
    let mut forward = Setup {
        platform: platform_of(Os::Windows),
        ..Setup::default()
    };
    forward.config.mouse.buttons_4_5 = "back_forward".into();
    w.desk(vec![
        ("Alpha", Setup::default()),
        ("Beta", Setup::default()),
        ("Gamma", forward),
    ])
    .await;
}

#[when("they press those buttons on another computer")]
async fn press_buttons_4_and_5(w: &mut CrossWorld) {
    let (alpha, beta, gamma) = (w.platform("Alpha"), w.platform("Beta"), w.platform("Gamma"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    for b in [4, 5] {
        alpha.send_input(button(b, true));
        alpha.send_input(button(b, false));
    }
    // On to Gamma, across Beta.
    alpha.send_input(motion(1920.0));
    injected(&gamma, "Gamma entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    for b in [4, 5] {
        alpha.send_input(button(b, true));
        alpha.send_input(button(b, false));
    }
}

#[then("the chosen behavior happens on that computer")]
async fn chosen_behavior_happens(w: &mut CrossWorld) {
    let (beta, gamma) = (w.platform("Beta"), w.platform("Gamma"));
    // Each chord ends with Alt up, so the Forward chord (the second) is done
    // when two have been injected; the first is only the Back chord's end.
    until("Forward on Gamma", || {
        let alt_ups = |i: &Injected| {
            *i == Injected::Key {
                usage: ALT,
                down: false,
            }
        };
        gamma.injected().iter().filter(|i| alt_ups(i)).count() == 2
    })
    .await;
    let buttons: Vec<_> = beta
        .injected()
        .into_iter()
        .filter(|i| matches!(i, Injected::Button { .. }))
        .collect();
    assert_eq!(
        buttons,
        [
            Injected::Button {
                button: 4,
                down: true
            },
            Injected::Button {
                button: 4,
                down: false
            },
            Injected::Button {
                button: 5,
                down: true
            },
            Injected::Button {
                button: 5,
                down: false
            },
        ],
        "passed through unchanged on Beta"
    );
    let keys: Vec<_> = gamma
        .injected()
        .into_iter()
        .filter(|i| matches!(i, Injected::Key { .. } | Injected::Button { .. }))
        .collect();
    let alt = |usage| {
        [
            Injected::Key {
                usage: ALT,
                down: true,
            },
            Injected::Key { usage, down: true },
            Injected::Key { usage, down: false },
            Injected::Key {
                usage: ALT,
                down: false,
            },
        ]
    };
    let mut want = alt(LEFT_ARROW).to_vec();
    want.extend(alt(RIGHT_ARROW));
    assert_eq!(keys, want, "Back then Forward on Gamma");
}

// ---- scenario 12: enable/disable ------------------------------------------------------------------------------

#[given("Mouser is active")]
async fn mouser_is_active(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    let status = w.lab.node("Alpha").status().await;
    assert_eq!(status.state, EngineState::Active);
}

#[when("the desk user presses the enable/disable hotkey")]
async fn presses_the_hotkey(w: &mut CrossWorld) {
    // The app's global hotkey calls `active.pause` while Mouser is active.
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActivePause, json!({}))
        .await;
}

#[then(
    "crossing stops (or resumes) on every computer and the tray icon shows the paused (or active) state"
)]
async fn crossing_stops_and_resumes(w: &mut CrossWorld) {
    let status = w.lab.node("Alpha").status().await;
    assert_eq!(tray_summary(&status).state, EngineState::Paused);
    let alpha = w.platform("Alpha");
    push_right(&alpha);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!alpha.is_grabbed(), "no crossing while paused");
    // The hotkey again resumes it.
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActiveResume, json!({}))
        .await;
    let status = w.lab.node("Alpha").status().await;
    assert_eq!(tray_summary(&status).state, EngineState::Active);
    push_right(&alpha);
    let beta = w.platform("Beta");
    injected(&beta, "the crossing", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

// ---- edge case: held at the crossing ------------------------------------------------------------------------------

#[given("a mouse button or modifier key is held down")]
async fn button_and_modifier_held(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    let alpha = w.platform("Alpha");
    alpha.send_input(key(0x0007_00E1, true));
    alpha.send_input(button(1, true));
}

#[when("the pointer crosses or the connection drops")]
async fn crosses_then_drops(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    alpha.send_input(key(A, true));
    injected(&beta, "a key held on Beta", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
    // The network between them goes dark.
    w.lab.node("Alpha").sim.set_blackhole(true);
}

#[then("no key or button stays stuck down on any computer")]
async fn nothing_stays_down(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    // At the crossing Alpha's applications were told the held keys went up.
    let local = alpha.injected();
    assert!(
        local.contains(&Injected::Key {
            usage: 0x0007_00E1,
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
    // When the connection dropped Beta let go of the key and Alpha took its input back.
    injected(&beta, "Beta to release the key", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    until("Alpha to take its input back", || !alpha.is_grabbed()).await;
    assert!(!beta.injected().contains(&Injected::Key {
        usage: 0x0007_00E1,
        down: true
    }));
}

// ---- edge case: local devices on the active computer ------------------------------------------------------------------

#[given("a mouse or keyboard is attached directly to the active computer")]
async fn devices_on_the_active_computer(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[when("it is used")]
async fn it_is_used(w: &mut CrossWorld) {
    let beta = w.platform("Beta");
    assert!(beta.send_input(key(A, true)));
    assert!(beta.send_input(key(A, false)));
}

#[then("its input acts on that computer, the same as the shared mouse and keyboard")]
async fn local_input_acts_locally(w: &mut CrossWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    assert_eq!(beta.local_delivered(), [key(A, true), key(A, false)]);
    assert!(!beta.is_grabbed());
    // The shared devices still work too.
    alpha.send_input(key(C, true));
    injected(&beta, "the shared keyboard", |i| {
        *i == Injected::Key {
            usage: C,
            down: true,
        }
    })
    .await;
}

// ---- edge case: docked or undocked --------------------------------------------------------------------------------------

#[given(
    "a computer joins with a different screen arrangement than last time (for example a laptop docked or undocked)"
)]
async fn different_arrangement(w: &mut CrossWorld) {
    w.default_desk(&["Alpha", "Beta"]).await;
    w.reviews_seen = w
        .lab
        .node("Alpha")
        .events
        .engine_events()
        .iter()
        .filter(|e| matches!(e, EngineEvent::ReviewOffered { .. }))
        .count();
    // Beta goes away, is docked, and comes back with two other displays.
    w.lab.node("Beta").stop().await;
    w.platform("Beta").set_displays(vec![
        screen("dock-1", (0.0, 0.0)),
        screen("dock-2", (1920.0, 0.0)),
    ]);
}

#[when("it joins")]
async fn it_joins(w: &mut CrossWorld) {
    w.lab.restart("Beta").await;
    w.displays = 3;
    w.settled().await;
}

#[then(
    "its current displays replace its previous ones in the layout and the desk owner is offered a review"
)]
async fn displays_replaced_and_review_offered(w: &mut CrossWorld) {
    let beta = w.lab.node("Beta").id();
    let skip = w.reviews_seen;
    w.lab
        .node("Alpha")
        .engine_event(
            "a review offer after the dock",
            skip,
            |e| matches!(e, EngineEvent::ReviewOffered { computer, .. } if *computer == beta),
        )
        .await;
    for name in ["Alpha", "Beta"] {
        let mut keys: Vec<_> = w
            .lab
            .node(name)
            .layout()
            .placements
            .into_iter()
            .filter(|p| p.display_id.computer == beta)
            .map(|p| p.display_id.key)
            .collect();
        keys.sort();
        assert_eq!(keys, ["dock-1", "dock-2"], "{name}'s layout");
    }
}
