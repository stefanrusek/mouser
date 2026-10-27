//! Step definitions for `features/us5.feature`: the pointer stays put when
//! the desk user does not mean to cross.

use std::fmt;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_platform::{Capability, Limitation, LocalInput, Os, PlatformInfo, SessionType};
use mouser_proto::control::{
    ConnectionState, Empty, JoinPending, Method, PendingJoin, ProblemKind, Topic,
};
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::{Value, json};

use crate::support::desk::Lab;

const WAIT: Duration = Duration::from_secs(20);

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct BlockWorld {
    pub lab: Lab,
}

impl fmt::Debug for BlockWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockWorld({:?})", self.lab.names())
    }
}

/// Push the shared mouse of a 1920x1080 computer against its right edge at
/// height `y`, or back away from it.
fn push_right_at(platform: &VirtualPlatform, y: f64) {
    platform.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (1919.0, y),
    });
}

fn push_left_at(platform: &VirtualPlatform, y: f64) {
    platform.send_input(LocalInput::Motion {
        dx: -1.0,
        dy: 0.0,
        abs: (0.0, y),
    });
}

fn back_away(platform: &VirtualPlatform) {
    platform.send_input(LocalInput::Motion {
        dx: -50.0,
        dy: 0.0,
        abs: (1000.0, 540.0),
    });
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Wait long enough for a crossing that would happen to have happened.
async fn a_moment() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

fn entered(platform: &VirtualPlatform) -> bool {
    platform
        .injected()
        .iter()
        .any(|i| matches!(i, Injected::MotionAbs { .. }))
}

impl BlockWorld {
    fn new() -> Self {
        Self { lab: Lab::new() }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    /// Alpha creates a desk and Beta joins it; both know each other's displays.
    async fn desk(&mut self) {
        self.lab.add("Alpha").await;
        self.lab.add("Beta").await;
        let _: mouser_proto::control::Desk = self
            .lab
            .node("Alpha")
            .call(Method::DeskCreate, json!({"name": "Home office"}))
            .await;
        let target = self.lab.node("Alpha").sim.clone();
        let address = self.lab.node("Beta").sim.route_to(&target).to_string();
        let _: JoinPending = self
            .lab
            .node("Beta")
            .call(Method::DeskJoin, json!({ "address": address }))
            .await;
        let value = self
            .lab
            .node("Alpha")
            .notification("a join request", Topic::JoinRequest, |p| {
                p["computer"]["name"] == "Beta"
            })
            .await;
        let request: PendingJoin = serde_json::from_value(value).unwrap();
        let _: Empty = self
            .lab
            .node("Alpha")
            .call(
                Method::JoinDecide,
                json!({"request_id": request.request_id, "approve": true}),
            )
            .await;
        for name in ["Alpha", "Beta"] {
            let node = self.lab.node(name);
            node.wait_status("both computers connected with their displays", |s| {
                s.computers.len() == 2
                    && s.computers.iter().all(|c| {
                        c.connection == ConnectionState::SelfComputer
                            || (c.connection == ConnectionState::Connected && c.displays > 0)
                    })
            })
            .await;
            until("the whole layout", || node.layout().placements.len() == 2).await;
        }
    }

    /// A blocked crossing is one nothing but the full-screen application stops.
    async fn crossing_is_blocked(&mut self) {
        self.desk().await;
        self.platform("Alpha").set_fullscreen(true);
    }

    /// Alpha is holding its devices back, i.e. the pointer left it.
    async fn crossed(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        until("the pointer to cross", || {
            alpha.is_grabbed() && entered(&beta)
        })
        .await;
    }

    async fn stayed(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        a_moment().await;
        assert!(!alpha.is_grabbed(), "the pointer stays on Alpha");
        assert!(!entered(&beta), "nothing reached Beta");
    }
}

// ---- scenario 1: corner blocking -------------------------------------------------------

#[given("corner blocking is on for an edge")]
async fn corner_blocking_is_on(w: &mut BlockWorld) {
    // On by default for every edge.
    w.desk().await;
}

#[when("the desk user pushes the pointer into a corner of that edge")]
async fn pushes_into_a_corner(w: &mut BlockWorld) {
    let alpha = w.platform("Alpha");
    push_right_at(&alpha, 1078.0);
}

#[then("it stays on the current display")]
async fn it_stays(w: &mut BlockWorld) {
    w.stayed().await;
}

// ---- scenario 2: full-screen blocking ---------------------------------------------------

#[given("a full-screen application is in front and full-screen blocking is on")]
async fn fullscreen_in_front(w: &mut BlockWorld) {
    w.crossing_is_blocked().await;
}

#[when("the desk user moves the pointer past an edge")]
async fn moves_past_an_edge(w: &mut BlockWorld) {
    push_right_at(&w.platform("Alpha"), 540.0);
}

// ---- scenario 3: double bump ------------------------------------------------------------

#[given("a crossing is blocked")]
async fn a_crossing_is_blocked(w: &mut BlockWorld) {
    w.crossing_is_blocked().await;
}

#[when("the desk user bumps the same edge twice in quick succession")]
async fn bumps_twice(w: &mut BlockWorld) {
    let alpha = w.platform("Alpha");
    push_right_at(&alpha, 540.0);
    back_away(&alpha);
    push_right_at(&alpha, 540.0);
}

#[then("the pointer crosses")]
async fn the_pointer_crosses(w: &mut BlockWorld) {
    w.crossed().await;
}

// ---- scenario 4: explanation ------------------------------------------------------------

#[when("it is the first blocked crossing since the desk user last saw this explanation")]
async fn first_blocked_crossing(w: &mut BlockWorld) {
    push_right_at(&w.platform("Alpha"), 540.0);
}

#[then("a notification says what blocked it and how to cross anyway or change the setting")]
async fn a_notification_explains(w: &mut BlockWorld) {
    let note = w
        .lab
        .node("Alpha")
        .notification("the explanation", Topic::Problem, |p| {
            p["kind"] == "crossing_blocked"
        })
        .await;
    let message = note["message"].as_str().unwrap();
    assert!(
        message.contains("full-screen"),
        "says what blocked it: {message}"
    );
    assert!(
        message.contains("twice"),
        "says how to cross anyway: {message}"
    );
    assert!(
        message.contains("arranger"),
        "says where to change it: {message}"
    );
    assert_eq!(note["fix"], "open_arranger_edge");
    let status = w.lab.node("Alpha").status().await;
    assert!(
        status
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::CrossingBlocked)
    );
}

// ---- scenario 5: per-edge setting -------------------------------------------------------

#[given("the desk owner opens an edge in the display arranger")]
async fn owner_opens_an_edge(w: &mut BlockWorld) {
    w.desk().await;
}

#[when("they change its corner blocking")]
async fn change_corner_blocking(w: &mut BlockWorld) {
    let alpha = w.lab.node("Alpha").id().to_string();
    let mut layout: Value = w.lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    let display = layout["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["display_id"]["computer"] == alpha.as_str())
        .unwrap()["display_id"]
        .clone();
    layout["edge_options"] = json!([{
        "display_id": display, "side": "right", "block_corners": false
    }]);
    let _: Value = w.lab.node("Alpha").call(Method::LayoutSet, layout).await;
    let beta = w.lab.node("Beta");
    until("Beta to hold the change", || {
        beta.layout().edge_options.len() == 1
    })
    .await;
}

#[then("the change applies to that edge")]
async fn applies_to_that_edge(w: &mut BlockWorld) {
    // Beta's left edge, which meets Alpha's, is another edge: still blocked.
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_left_at(&beta, 1.0);
    a_moment().await;
    assert!(!beta.is_grabbed(), "Beta's left edge still blocks corners");
    // Alpha's right edge crosses in its corner.
    push_right_at(&alpha, 1.0);
    w.crossed().await;
}

// ---- accepted limitation: Wayland -------------------------------------------------------

const WAYLAND: &str = "Full-screen blocking is unavailable on Wayland.";

#[given("full-screen blocking (Story 5 sc. 2) is unavailable on Wayland")]
async fn unavailable_on_wayland(w: &mut BlockWorld) {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os: Os::Linux,
        version: "test".into(),
        session: SessionType::Wayland,
        limitations: vec![Limitation {
            capability: Capability::Capture,
            text: WAYLAND.into(),
        }],
    });
    w.lab
        .add_with(
            "Linux",
            crate::support::desk::Setup {
                platform,
                ..Default::default()
            },
        )
        .await;
}

#[when("the desk user looks at the app on a Wayland computer")]
async fn looks_at_the_app(_w: &mut BlockWorld) {
    // The app renders `status`; the next step reads it.
}

#[then("the app states that full-screen blocking is unavailable")]
async fn app_states_it(w: &mut BlockWorld) {
    let status = w.lab.node("Linux").status().await;
    assert!(status.computers[0].limitations.iter().any(|l| l == WAYLAND));
    assert_eq!(status.problems, [], "a limitation is not a problem");
}
