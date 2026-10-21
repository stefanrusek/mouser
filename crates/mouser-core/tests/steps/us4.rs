//! Step definitions for `features/us4.feature`: the owner arranges displays
//! to match the physical desk.

use std::fmt;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_core::EngineEvent;
use mouser_platform::{LocalDisplay, LocalInput, Platform};
use mouser_proto::control::{ConnectionState, Empty, JoinPending, Method, PendingJoin, Topic};
use mouser_proto::model::Layout;
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::{Value, json};

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct ArrangeWorld {
    pub lab: Lab,
    /// Displays on the desk, for knowing when the layout is complete.
    displays: usize,
    /// Review offers Alpha had seen before the step under test.
    reviews_seen: usize,
    /// The layout before a display was removed.
    before_removal: Value,
}

impl fmt::Debug for ArrangeWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ArrangeWorld({:?})", self.lab.names())
    }
}

/// A 1920x1080 display of 527x296 mm.
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

/// The same size on the desk at twice the density: 1280x720 points.
fn retina(key: &str) -> LocalDisplay {
    LocalDisplay {
        pixel_size: (2560, 1440),
        logical_size: (1280.0, 720.0),
        scale: 2.0,
        ..screen(key, (0.0, 0.0))
    }
}

fn setup_of(platform: VirtualPlatform) -> Setup {
    Setup {
        platform,
        ..Setup::default()
    }
}

fn platform_with(displays: Vec<LocalDisplay>) -> Setup {
    let platform = VirtualPlatform::new();
    platform.set_displays(displays);
    setup_of(platform)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

/// Push the shared mouse of a 1920x1080 computer off one of its edges.
fn push(platform: &VirtualPlatform, dx: f64, dy: f64, abs: (f64, f64)) {
    platform.send_input(LocalInput::Motion { dx, dy, abs });
}

fn push_right(platform: &VirtualPlatform) {
    push(platform, 1.0, 0.0, (1919.0, 540.0));
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn injected(platform: &VirtualPlatform, what: &str, pred: impl Fn(&Injected) -> bool) {
    until(what, || platform.injected().iter().any(&pred)).await;
}

/// Wait long enough for a crossing that would happen to have happened.
async fn a_moment() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

impl ArrangeWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            displays: 0,
            reviews_seen: 0,
            before_removal: Value::Null,
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

    /// Wait until every computer sees the others connected and the whole
    /// layout.
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

    /// A desk of computers in this order, the first creating it.
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

    /// What `layout.get` says on `name`.
    async fn arranger(&mut self, name: &str) -> Value {
        self.lab.node(name).call(Method::LayoutGet, json!({})).await
    }

    /// `layout.set` on `name`; returns once every computer holds the result.
    async fn arrange(&mut self, name: &str, request: Value) {
        let done: Value = self.lab.node(name).call(Method::LayoutSet, request).await;
        let layout: Layout = serde_json::from_value(done).expect("the result is a layout");
        for other in self.lab.names() {
            let node = self.lab.node(&other);
            until("every computer to hold the layout", || {
                node.layout() == layout
            })
            .await;
        }
    }
}

/// `layout` with the placement of `computer`'s displays moved to `(x, y)`.
fn move_computer(layout: &mut Value, computer: &str, x: i32, y: i32) {
    for placement in layout["placements"].as_array_mut().unwrap() {
        if placement["display_id"]["computer"] == computer {
            placement["x"] = json!(x);
            placement["y"] = json!(y);
        }
    }
}

// ---- scenario 1: drag a display ---------------------------------------------------

#[given("the arranger is open")]
async fn the_arranger_is_open(w: &mut ArrangeWorld) {
    // Alpha | Beta | Gamma, left to right.
    w.desk(vec![
        ("Alpha", Setup::default()),
        ("Beta", Setup::default()),
        ("Gamma", platform_with(vec![retina("fine")])),
    ])
    .await;
}

#[when("the owner drags a display to a new position")]
async fn drags_a_display(w: &mut ArrangeWorld) {
    // Gamma goes below Alpha, where it sits on the real desk.
    let gamma = w.lab.node("Gamma").id().to_string();
    let mut layout = w.arranger("Alpha").await;
    move_computer(&mut layout, &gamma, 0, 296);
    w.arrange("Alpha", layout).await;
}

#[then("the pointer crosses at the edges that position implies, on every computer on the desk")]
async fn crosses_at_the_new_edges(w: &mut ArrangeWorld) {
    // Each leg: the controller pushes at its edge, enters the target, and
    // comes back by moving `back` the other way.
    struct Leg {
        controller: &'static str,
        edge: (f64, f64, (f64, f64)),
        target: &'static str,
        back: (f64, f64),
    }
    let legs = [
        Leg {
            controller: "Alpha",
            edge: (0.0, 1.0, (960.0, 1079.0)),
            target: "Gamma",
            back: (0.0, -5000.0),
        },
        Leg {
            controller: "Gamma",
            edge: (0.0, -1.0, (640.0, 0.0)),
            target: "Alpha",
            back: (0.0, 5000.0),
        },
        Leg {
            controller: "Alpha",
            edge: (1.0, 0.0, (1919.0, 540.0)),
            target: "Beta",
            back: (-5000.0, 0.0),
        },
        Leg {
            controller: "Beta",
            edge: (-1.0, 0.0, (0.0, 540.0)),
            target: "Alpha",
            back: (5000.0, 0.0),
        },
    ];
    for Leg {
        controller,
        edge: (dx, dy, abs),
        target,
        back,
    } in legs
    {
        let (c, t) = (w.platform(controller), w.platform(target));
        t.take_injected();
        push(&c, dx, dy, abs);
        injected(&t, &format!("{controller} to enter {target}"), |i| {
            matches!(i, Injected::MotionAbs { .. })
        })
        .await;
        assert!(c.is_grabbed(), "{controller} holds its devices");
        push(&c, back.0, back.1, (640.0, 360.0));
        until(&format!("{controller} to get control back"), || {
            !c.is_grabbed()
        })
        .await;
    }
    // The edge Beta had with Gamma is gone.
    let (beta, gamma) = (w.platform("Beta"), w.platform("Gamma"));
    gamma.take_injected();
    push_right(&beta);
    a_moment().await;
    assert!(!beta.is_grabbed(), "Beta no longer borders Gamma");
    assert!(gamma.injected().is_empty());
}

// ---- scenario 2: display details ------------------------------------------------------

#[when("the owner points at a display")]
async fn points_at_a_display(w: &mut ArrangeWorld) {
    // The arranger already holds the details; they are read in the next step.
    let layout = w.arranger("Alpha").await;
    assert_eq!(layout["displays"].as_array().unwrap().len(), 3);
}

fn detail<'a>(layout: &'a Value, computer: &str) -> &'a Value {
    layout["displays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["computer_name"] == computer)
        .unwrap_or_else(|| panic!("no display of {computer}"))
}

#[then(
    "it shows the display's resolution, scale, owning computer, and whether that computer is online, or when it was last seen if offline"
)]
async fn shows_the_details(w: &mut ArrangeWorld) {
    let layout = w.arranger("Alpha").await;
    let gamma = detail(&layout, "Gamma");
    assert_eq!(gamma["pixel_size"], json!([2560, 1440]));
    assert_eq!(gamma["scale"], 2.0);
    assert_eq!(gamma["online"], true);
    assert_eq!(gamma["last_seen"], Value::Null);
    let beta = detail(&layout, "Beta");
    assert_eq!(beta["pixel_size"], json!([1920, 1080]));
    assert_eq!(beta["scale"], 1.0);
    // Gamma goes away: it shows as not online, and since when.
    w.lab.node("Gamma").stop().await;
    let end = Instant::now() + WAIT;
    let layout = loop {
        let layout = w.arranger("Alpha").await;
        if detail(&layout, "Gamma")["online"] == false {
            break layout;
        }
        assert!(Instant::now() < end, "Gamma never showed as offline");
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert!(detail(&layout, "Gamma")["last_seen"].is_string());
    assert_eq!(detail(&layout, "Beta")["online"], true);
}

// ---- scenario 3: remove a display --------------------------------------------------------------

/// `layout` without the placement of Beta's display `key`, and a request to
/// include or remove it.
fn inclusion(mut layout: Value, beta: &str, key: &str, included: bool) -> Value {
    if !included {
        layout["placements"]
            .as_array_mut()
            .unwrap()
            .retain(|p| p["display_id"]["key"] != key);
    }
    layout["displays"] = json!([{
        "display_id": {"computer": beta, "key": key},
        "included": included
    }]);
    layout
}

#[given("a display should not be part of the desk")]
async fn a_display_should_not_be_part_of_the_desk(w: &mut ArrangeWorld) {
    // Alpha | b1 | b2.
    w.desk(vec![
        ("Alpha", Setup::default()),
        (
            "Beta",
            platform_with(vec![screen("b1", (0.0, 0.0)), screen("b2", (1920.0, 0.0))]),
        ),
    ])
    .await;
}

#[when("the owner removes it from the layout")]
async fn removes_it(w: &mut ArrangeWorld) {
    let beta = w.lab.node("Beta").id().to_string();
    let full = w.arranger("Beta").await;
    w.before_removal = full.clone();
    w.arrange("Beta", inclusion(full, &beta, "b2", false)).await;
}

#[then("the pointer never crosses onto it, and the owner can undo the removal")]
async fn never_crosses_onto_it(w: &mut ArrangeWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(
        &beta,
        "Beta's first display",
        |i| matches!(i, Injected::MotionAbs { x, .. } if near(*x, 0.0)),
    )
    .await;
    // All the way across b1 and on: the pointer is held at its right edge.
    for _ in 0..3 {
        push(&alpha, 2000.0, 0.0, (960.0, 540.0));
    }
    injected(
        &beta,
        "the pointer at b1's right edge",
        |i| matches!(i, Injected::MotionAbs { x, .. } if *x > 1900.0),
    )
    .await;
    a_moment().await;
    assert!(alpha.is_grabbed(), "the pointer stays on Beta");
    assert!(
        beta.injected()
            .iter()
            .all(|i| !matches!(i, Injected::MotionAbs { x, .. } if *x >= 1920.0)),
        "{:?}",
        beta.injected()
    );
    push(&alpha, -9000.0, 0.0, (960.0, 540.0));
    until("Alpha to get control back", || !alpha.is_grabbed()).await;

    // Undo: the display is back where it was.
    let id = w.lab.node("Beta").id().to_string();
    let restored = inclusion(w.before_removal.clone(), &id, "b2", true);
    w.arrange("Beta", restored).await;
    // Alpha learns that the display is back along with its placement.
    loop {
        let seen = w.arranger("Alpha").await;
        let b2 = seen["displays"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["display_id"]["key"] == "b2")
            .unwrap()
            .clone();
        if b2["included"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    beta.take_injected();
    push_right(&alpha);
    injected(
        &beta,
        "Beta's first display again",
        |i| matches!(i, Injected::MotionAbs { x, .. } if near(*x, 0.0)),
    )
    .await;
    for _ in 0..3 {
        push(&alpha, 2000.0, 0.0, (960.0, 540.0));
    }
    injected(
        &beta,
        "Beta's second display",
        |i| matches!(i, Injected::MotionAbs { x, .. } if *x >= 1920.0),
    )
    .await;
}

// ---- scenario 4: equalized dimensions -----------------------------------------------------------

#[given("adjoining displays differ in physical size")]
async fn displays_differ_in_size(w: &mut ArrangeWorld) {
    // Beta's display is as wide but only half as tall as Alpha's.
    let mut short = screen("short", (0.0, 0.0));
    short.physical_size_mm = Some((527.0, 148.0));
    w.desk(vec![
        ("Alpha", Setup::default()),
        ("Beta", platform_with(vec![short])),
    ])
    .await;
}

#[when("the owner turns on equalized dimensions")]
async fn turns_on_equalized_dimensions(w: &mut ArrangeWorld) {
    // Half way down Alpha is below the end of Beta's shorter edge.
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    a_moment().await;
    assert!(
        !alpha.is_grabbed(),
        "off, the longer edge ends where the shorter one does"
    );
    assert!(beta.injected().is_empty());
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(
            Method::ConfigSet,
            json!({"key": "crossing.equalize_dimensions", "value": true}),
        )
        .await;
    for name in ["Alpha", "Beta"] {
        let node = w.lab.node(name);
        until("the setting on every computer", || {
            node.layout().equalize_dimensions
        })
        .await;
    }
}

#[then("crossing maps the full length of one edge onto the full length of the other")]
async fn maps_full_edge_to_full_edge(w: &mut ArrangeWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    // Half way down Alpha is half way down Beta, and the bottom is the bottom.
    for (at, lands) in [(540.0, 540.0), (1079.0, 1079.0), (0.0, 0.0)] {
        beta.take_injected();
        push(&alpha, 1.0, 0.0, (1919.0, at));
        injected(
            &beta,
            "the pointer on Beta",
            |i| matches!(i, Injected::MotionAbs { x, y } if near(*x, 0.0) && (y - lands).abs() < 2.0),
        )
        .await;
        push(&alpha, -9000.0, 0.0, (960.0, 540.0));
        until("Alpha to get control back", || !alpha.is_grabbed()).await;
    }
}

// ---- edge case: every display of a computer removed ---------------------------------------------------

#[given("the desk owner removes every display of a computer from the layout")]
async fn removes_every_display(w: &mut ArrangeWorld) {
    w.desk(vec![
        ("Alpha", Setup::default()),
        ("Beta", Setup::default()),
    ])
    .await;
    let beta = w.lab.node("Beta").id().to_string();
    let mut layout = w.arranger("Beta").await;
    layout["placements"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["display_id"]["computer"] != beta.as_str());
    layout["displays"] = json!([{
        "display_id": {"computer": beta, "key": "virtual-0"},
        "included": false
    }]);
    w.arrange("Beta", layout).await;
}

#[when("the desk is in use")]
async fn the_desk_is_in_use(w: &mut ArrangeWorld) {
    let alpha = w.platform("Alpha");
    push_right(&alpha);
    a_moment().await;
}

#[then(
    "that computer stays a desk member, receives clipboard content, and the pointer never crosses onto it"
)]
async fn stays_a_member(w: &mut ArrangeWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    assert!(!alpha.is_grabbed());
    assert!(beta.injected().is_empty());
    // Still a connected member of the desk, which is what clipboard offers go to.
    for name in ["Alpha", "Beta"] {
        let status = w.lab.node(name).status().await;
        assert_eq!(status.computers.len(), 2, "{name}");
        assert!(
            status.computers.iter().all(|c| matches!(
                c.connection,
                ConnectionState::SelfComputer | ConnectionState::Connected
            )),
            "{name}"
        );
    }
}

// ---- edge case: docked or undocked -------------------------------------------------------------------------

#[given(
    "a computer joins with a different screen arrangement than last time (for example a laptop docked or undocked)"
)]
async fn different_arrangement(w: &mut ArrangeWorld) {
    w.desk(vec![
        ("Alpha", Setup::default()),
        ("Beta", Setup::default()),
    ])
    .await;
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
async fn it_joins(w: &mut ArrangeWorld) {
    w.lab.restart("Beta").await;
    w.displays = 3;
    w.settled().await;
}

#[then(
    "its current displays replace its previous ones in the layout and the desk owner is offered a review"
)]
async fn displays_replaced_and_review_offered(w: &mut ArrangeWorld) {
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
