//! Step definitions for `features/us3.feature`: the desk keeps working
//! through sleep, network changes and VPNs. Every impairment is applied
//! through `SimNet`: a computer sleeps and wakes, its interfaces come and go
//! (which makes its platform report a network change and moves its address),
//! or the network drops everything to and from it.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_platform::{InterfaceKind, LocalInput, NetInterface};
use mouser_proto::control::{
    ConnectionState, Empty, JoinPending, Method, PendingJoin, Status, Topic,
};
use mouser_testkit::{Injected, SimNode, VirtualPlatform};
use serde_json::json;

use crate::support::desk::Lab;

const WAIT: Duration = Duration::from_secs(20);

/// SC-008: a computer that woke or changed network can be crossed to this
/// soon.
pub const CROSS_WITHIN: Duration = Duration::from_secs(5);

/// How long after the user moves the pointer past an edge the crossing may
/// take, once the other computer is connected (SC-007: no visible pause).
const CROSSING_LATENCY: Duration = Duration::from_millis(500);

/// Asleep longer than the transport's 3 s idle timeout, so the connections
/// to the sleeper are lost, not merely paused (research R2).
const ASLEEP: Duration = Duration::from_millis(4500);

const SHIFT: u32 = 0x0007_00E1;
const C_KEY: u32 = 0x0007_0006;

/// Start the computers called `names` and put them on one desk, the first
/// creating it and the others joining by address with one approval each;
/// returns once every computer sees every other connected with the whole
/// layout.
pub async fn form_desk(lab: &mut Lab, names: &[&str]) {
    for name in names {
        lab.add(name).await;
    }
    let _: mouser_proto::control::Desk = lab
        .node(names[0])
        .call(Method::DeskCreate, json!({"name": "Home office"}))
        .await;
    for joiner in &names[1..] {
        let target = lab.node(names[0]).sim.clone();
        let address = lab.node(joiner).sim.route_to(&target).to_string();
        let _: JoinPending = lab
            .node(joiner)
            .call(Method::DeskJoin, json!({ "address": address }))
            .await;
        let value = lab
            .node(names[0])
            .notification("a join request", Topic::JoinRequest, |p| {
                p["computer"]["name"] == *joiner
            })
            .await;
        let request: PendingJoin = serde_json::from_value(value).unwrap();
        let _: Empty = lab
            .node(names[0])
            .call(
                Method::JoinDecide,
                json!({"request_id": request.request_id, "approve": true}),
            )
            .await;
    }
    for name in names {
        let node = lab.node(name);
        node.wait_status("every computer connected with its displays", |s| {
            s.computers.len() == names.len()
                && s.computers.iter().all(|c| {
                    c.connection == ConnectionState::SelfComputer
                        || (c.connection == ConnectionState::Connected && c.displays > 0)
                })
        })
        .await;
        until("the whole layout", || {
            node.layout().placements.len() == names.len()
        })
        .await;
    }
}

/// Wait until `what` holds.
pub async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Alpha's display is left of Beta's: the shared mouse leaves Alpha through
/// its right edge and Beta through its left edge.
pub fn push_toward(from: &str, platform: &VirtualPlatform) {
    let (dx, x) = if from == "Alpha" {
        (1.0, 1919.0)
    } else {
        (-1.0, 0.0)
    };
    platform.send_input(LocalInput::Motion {
        dx,
        dy: 0.0,
        abs: (x, 540.0),
    });
}

/// The shared mouse of `controller`, whose pointer is on the other computer,
/// goes back across the edge it left through.
pub fn push_back(controller: &str, platform: &VirtualPlatform) {
    let dx = if controller == "Alpha" { -1.0 } else { 1.0 };
    platform.send_input(LocalInput::Motion {
        dx,
        dy: 0.0,
        abs: (960.0, 540.0),
    });
}

fn other(name: &str) -> &'static str {
    if name == "Alpha" { "Beta" } else { "Alpha" }
}

fn interface(name: &str, kind: InterfaceKind, addr: [u8; 4]) -> NetInterface {
    NetInterface {
        name: name.into(),
        kind,
        addrs: vec![std::net::IpAddr::from(addr)],
    }
}

/// Keys and buttons still down after `injected`.
fn held(injected: &[Injected]) -> (BTreeSet<u32>, BTreeSet<u8>) {
    let (mut keys, mut buttons) = (BTreeSet::new(), BTreeSet::new());
    for i in injected {
        match i {
            Injected::Key { usage, down: true } => keys.insert(*usage),
            Injected::Key { usage, down: false } => keys.remove(usage),
            Injected::Button { button, down: true } => buttons.insert(*button),
            Injected::Button {
                button,
                down: false,
            } => buttons.remove(button),
            _ => false,
        };
    }
    (keys, buttons)
}

/// Someone typing without pause on one computer's keyboard, while the
/// network to the computer being typed on keeps changing its delay.
pub struct Typing {
    stop: Arc<AtomicBool>,
    sent: Arc<AtomicU32>,
    task: tokio::task::JoinHandle<()>,
}

impl Typing {
    pub fn start(keyboard: VirtualPlatform, network: SimNode) -> Self {
        let (stop, sent) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicU32::new(0)),
        );
        let (flag, count) = (stop.clone(), sent.clone());
        let task = tokio::spawn(async move {
            let mut n = 0u32;
            while !flag.load(Ordering::Relaxed) {
                let usage = key_of(n);
                keyboard.send_input(LocalInput::Key { usage, down: true });
                keyboard.send_input(LocalInput::Key { usage, down: false });
                n += 1;
                count.store(n, Ordering::Relaxed);
                // Packets overtake each other when the delay drops.
                if n.is_multiple_of(75) {
                    network.set_delay(if (n / 75) % 2 == 1 {
                        Duration::from_millis(25)
                    } else {
                        Duration::ZERO
                    });
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            network.set_delay(Duration::ZERO);
        });
        Self { stop, sent, task }
    }

    /// How many keystrokes have been typed so far.
    pub fn typed(&self) -> u32 {
        self.sent.load(Ordering::Relaxed)
    }

    /// Stop typing; how many keystrokes (a press and a release each) were typed.
    pub async fn finish(self) -> u32 {
        self.stop.store(true, Ordering::Relaxed);
        self.task.await.unwrap();
        self.sent.load(Ordering::Relaxed)
    }
}

/// The key typed as the `n`th keystroke.
fn key_of(n: u32) -> u32 {
    0x0007_0004 + n % 26
}

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct LinkWorld {
    pub lab: Lab,
    pub typing: Option<Typing>,
    /// The computer the impairment hit.
    subject: String,
    /// When the impairment ended (wake) or happened (network change).
    since: Option<Instant>,
    /// How many `Status` notifications Alpha and Beta had when the desk was
    /// complete.
    status_marks: (usize, usize),
    /// How many join requests Alpha and Beta had when the desk was complete.
    join_marks: (usize, usize),
}

impl fmt::Debug for LinkWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LinkWorld({:?})", self.lab.names())
    }
}

impl LinkWorld {
    pub fn new() -> Self {
        Self {
            lab: Lab::new(),
            typing: None,
            subject: String::new(),
            since: None,
            status_marks: (0, 0),
            join_marks: (0, 0),
        }
    }

    pub fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    pub fn sim(&mut self, name: &str) -> SimNode {
        self.lab.node(name).sim.clone()
    }

    pub async fn desk(&mut self) {
        form_desk(&mut self.lab, &["Alpha", "Beta"]).await;
        self.status_marks = (
            self.lab.node("Alpha").events.all(Topic::Status).len(),
            self.lab.node("Beta").events.all(Topic::Status).len(),
        );
        self.join_marks = (
            self.lab.node("Alpha").events.all(Topic::JoinRequest).len(),
            self.lab.node("Beta").events.all(Topic::JoinRequest).len(),
        );
    }

    /// Alpha's mouse and keyboard now drive Beta.
    pub async fn alpha_drives_beta(&mut self) {
        self.desk().await;
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        push_toward("Alpha", &alpha);
        until("the pointer on Beta", || {
            beta.injected()
                .iter()
                .any(|i| matches!(i, Injected::MotionAbs { .. }))
        })
        .await;
        beta.take_injected();
    }

    /// The computer with the higher id accepts the connection (the lower
    /// dials), the harder side to move.
    fn acceptor(&mut self) -> &'static str {
        if self.lab.node("Alpha").id() < self.lab.node("Beta").id() {
            "Beta"
        } else {
            "Alpha"
        }
    }

    /// Wait until `name` shows `other` as `state`.
    pub async fn sees(&mut self, name: &str, other_name: &str, state: ConnectionState) -> Status {
        self.lab
            .node(name)
            .wait_status(&format!("{other_name} to be {state:?}"), |s| {
                s.computers
                    .iter()
                    .any(|c| c.name == other_name && c.connection == state)
            })
            .await
    }

    /// The user keeps moving the pointer from `from` toward the other
    /// computer until it crosses, for at most `limit` from `since`.
    async fn keep_moving_toward(&mut self, from: &str, limit: Duration) {
        let platform = self.platform(from);
        let since = self.since.expect("the impairment has happened");
        while !platform.is_grabbed() {
            assert!(
                since.elapsed() < limit,
                "the pointer could not cross within {limit:?}"
            );
            push_toward(from, &platform);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The pointer reached the other computer and comes back, then crosses
    /// the other way: crossing works in both directions.
    async fn crossing_works_both_ways(&mut self, from: &str) {
        let to = other(from);
        let (platform, target) = (self.platform(from), self.platform(to));
        until("the pointer on the other computer", || {
            target
                .injected()
                .iter()
                .any(|i| matches!(i, Injected::MotionAbs { .. }))
        })
        .await;
        let warps = platform.warps().len();
        push_back(from, &platform);
        until("the pointer back", || platform.warps().len() > warps).await;
        assert!(!platform.is_grabbed());
        // And from the other computer toward this one.
        let before = platform.injected().len();
        push_toward(to, &target);
        until("the pointer arriving the other way", || {
            platform.injected().len() > before
        })
        .await;
        assert!(target.is_grabbed());
    }

    /// Stop the typist: every keystroke typed arrived on Beta, in order and
    /// none missing, Alpha's mouse and keyboard still drive Beta, and no
    /// session ended meanwhile.
    pub async fn finish_typing_and_check(&mut self) {
        // Enough keystrokes to span many delay changes: a coarse OS timer
        // (Windows rounds the typist's 2 ms pause up to ~16 ms) types fewer
        // in the same time, so keep typing until there are.
        let typing = self.typing.take().expect("typing");
        until("more than 500 keystrokes typed", || typing.typed() > 500).await;
        let typed = typing.finish().await;
        let beta = self.platform("Beta");
        let keys = |platform: &VirtualPlatform| -> Vec<Injected> {
            platform
                .injected()
                .into_iter()
                .filter(|i| matches!(i, Injected::Key { .. }))
                .collect()
        };
        until("every keystroke to arrive", || {
            keys(&beta).len() >= typed as usize * 2
        })
        .await;
        let expected: Vec<Injected> = (0..typed)
            .flat_map(|n| {
                let usage = key_of(n);
                [
                    Injected::Key { usage, down: true },
                    Injected::Key { usage, down: false },
                ]
            })
            .collect();
        assert_eq!(keys(&beta), expected, "in order, none dropped");
        self.assert_sessions_never_ended();
        let alpha = self.lab.node("Alpha").status().await;
        let beta_id = self.lab.node("Beta").id();
        assert_eq!(alpha.active_computer, Some(beta_id));
    }

    /// No desk-mate was ever shown as anything but connected since the desk
    /// was complete: the session did not end.
    pub fn assert_sessions_never_ended(&mut self) {
        for (name, mark) in [
            ("Alpha", self.status_marks.0),
            ("Beta", self.status_marks.1),
        ] {
            let notes = self.lab.node(name).events.all(Topic::Status);
            for note in &notes[mark..] {
                let status: Status = serde_json::from_value(note.clone()).unwrap();
                for c in &status.computers {
                    assert!(
                        matches!(
                            c.connection,
                            ConnectionState::SelfComputer | ConnectionState::Connected
                        ),
                        "{name} saw {} as {:?}",
                        c.name,
                        c.connection
                    );
                }
            }
        }
    }
}

// ---- scenario 1: continuous typing ---------------------------------------------

#[given("the desk user is typing continuously on another computer")]
async fn typing_continuously(w: &mut LinkWorld) {
    w.alpha_drives_beta().await;
    let (alpha, beta) = (w.platform("Alpha"), w.sim("Beta"));
    w.typing = Some(Typing::start(alpha, beta));
}

#[when("they keep typing")]
async fn keep_typing(_w: &mut LinkWorld) {
    // Longer than the 3 s idle timeout: nothing that merely ends a quiet
    // connection may end this one.
    tokio::time::sleep(Duration::from_secs(5)).await;
}

#[then("every keystroke arrives in order with none dropped and the session does not end")]
async fn every_keystroke_arrives(w: &mut LinkWorld) {
    w.finish_typing_and_check().await;
}

// ---- scenario 2: wake from sleep -------------------------------------------------

#[given("a computer on the desk wakes from sleep")]
async fn wakes_from_sleep(w: &mut LinkWorld) {
    w.desk().await;
    w.subject = "Beta".into();
    let beta = w.sim("Beta");
    beta.sleep();
    // Both sides lose the connection while it sleeps.
    w.sees("Alpha", "Beta", ConnectionState::Reconnecting).await;
    tokio::time::sleep(ASLEEP.saturating_sub(Duration::from_secs(3))).await;
    beta.wake();
    w.since = Some(Instant::now());
}

#[when("the desk user moves the pointer toward it")]
async fn moves_toward_it(w: &mut LinkWorld) {
    w.keep_moving_toward("Alpha", CROSS_WITHIN).await;
}

#[then("the pointer crosses onto it without the user restarting, re-joining, or opening Mouser")]
async fn crosses_without_action(w: &mut LinkWorld) {
    w.crossing_works_both_ways("Alpha").await;
    // Nobody restarted, re-joined or was asked anything.
    for (name, mark) in [("Alpha", w.join_marks.0), ("Beta", w.join_marks.1)] {
        assert_eq!(
            w.lab.node(name).events.all(Topic::JoinRequest).len(),
            mark,
            "{name} was asked to approve a join"
        );
        let status = w.lab.node(name).status().await;
        let desk = status.desk.expect("still on the desk");
        assert_eq!(desk.pending_join, None);
    }
}

// ---- scenario 3: a new address ---------------------------------------------------

#[given("a computer switches between Wi-Fi and Ethernet or receives a new network address")]
async fn switches_network(w: &mut LinkWorld) {
    w.desk().await;
    let subject = w.acceptor();
    w.subject = subject.into();
    let sim = w.sim(subject);
    // Wi-Fi off, Ethernet on, on another network.
    assert!(sim.remove_interface("eth0"));
    sim.add_interface(interface(
        "en7",
        InterfaceKind::PhysicalLan,
        [192, 168, 77, 5],
    ));
    w.since = Some(Instant::now());
}

#[when("the desk user crosses to it afterwards")]
async fn crosses_to_it(w: &mut LinkWorld) {
    let from = other(&w.subject.clone());
    w.keep_moving_toward(from, CROSS_WITHIN).await;
}

#[then("crossing works without any action in Mouser")]
async fn crossing_works(w: &mut LinkWorld) {
    let from = other(&w.subject.clone());
    w.crossing_works_both_ways(from).await;
}

// ---- scenario 4: VPN, virtual machine, container -----------------------------------

#[given(
    "a VPN connection, virtual machine, or container network is started or stopped on any desk computer"
)]
async fn virtual_networks(w: &mut LinkWorld) {
    w.desk().await;
    w.subject = "Beta".into();
    for name in ["Alpha", "Beta"] {
        let sim = w.sim(name);
        sim.add_interface(interface("utun3", InterfaceKind::Virtual, [10, 8, 0, 2]));
        sim.add_interface(interface(
            "vmnet8",
            InterfaceKind::Virtual,
            [192, 168, 64, 1],
        ));
        sim.add_interface(interface(
            "docker0",
            InterfaceKind::Virtual,
            [172, 17, 0, 1],
        ));
    }
    // The VPN is turned off again on one of them.
    assert!(w.sim("Beta").remove_interface("utun3"));
    w.since = Some(Instant::now());
}

#[when("the desk user crosses to it")]
async fn crosses_to_beta(w: &mut LinkWorld) {
    w.keep_moving_toward("Alpha", CROSS_WITHIN).await;
}

#[then("crossing works without any action or setting in Mouser")]
async fn crossing_works_without_settings(w: &mut LinkWorld) {
    w.crossing_works_both_ways("Alpha").await;
}

// ---- scenario 5: unreachable with the pointer --------------------------------------

#[given("a desk computer becomes unreachable while it has the pointer")]
async fn unreachable_with_the_pointer(w: &mut LinkWorld) {
    w.alpha_drives_beta().await;
    w.sim("Beta").set_blackhole(true);
}

#[when("the connection is lost")]
async fn connection_is_lost(w: &mut LinkWorld) {
    w.sees("Alpha", "Beta", ConnectionState::Reconnecting).await;
}

#[then("the pointer and keyboard return to the computer the desk user is physically at")]
async fn control_returns(w: &mut LinkWorld) {
    let alpha = w.platform("Alpha");
    assert!(!alpha.is_grabbed(), "Alpha's mouse is no longer taken");
    assert_eq!(
        alpha.warps(),
        [(1919.0, 540.0)],
        "the pointer is back on Alpha"
    );
    assert_eq!(w.lab.node("Alpha").status().await.active_computer, None);
    alpha.send_input(LocalInput::Key {
        usage: C_KEY,
        down: true,
    });
    alpha.send_input(LocalInput::Key {
        usage: C_KEY,
        down: false,
    });
    let delivered = alpha.local_delivered();
    assert_eq!(
        delivered[delivered.len() - 2..],
        [
            LocalInput::Key {
                usage: C_KEY,
                down: true
            },
            LocalInput::Key {
                usage: C_KEY,
                down: false
            }
        ],
        "keys now type on Alpha"
    );
}

// ---- edge case: unreachable when the pointer approaches ------------------------------

#[given("a desk computer is unreachable")]
async fn computer_is_unreachable(w: &mut LinkWorld) {
    w.desk().await;
    w.sim("Beta").set_blackhole(true);
    w.sees("Alpha", "Beta", ConnectionState::Offline).await;
}

#[when("the pointer approaches its display")]
async fn pointer_approaches(w: &mut LinkWorld) {
    let alpha = w.platform("Alpha");
    push_toward("Alpha", &alpha);
    push_toward("Alpha", &alpha);
    tokio::time::sleep(Duration::from_millis(300)).await;
}

#[then(
    "the pointer stays on the current display and the health panel shows that computer as offline"
)]
async fn pointer_stays(w: &mut LinkWorld) {
    let alpha = w.platform("Alpha");
    assert!(!alpha.is_grabbed());
    assert!(alpha.warps().is_empty(), "the pointer did not move");
    let status = w.lab.node("Alpha").status().await;
    assert_eq!(status.active_computer, None);
    let beta = status.computers.iter().find(|c| c.name == "Beta").unwrap();
    assert_eq!(beta.connection, ConnectionState::Offline);
    assert!(beta.last_seen.is_some(), "it says when Beta was last seen");
}

// ---- edge case: a key held when the connection drops ---------------------------------

#[given("a mouse button or modifier key is held down")]
async fn button_and_modifier_held(w: &mut LinkWorld) {
    w.alpha_drives_beta().await;
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    alpha.send_input(LocalInput::Key {
        usage: SHIFT,
        down: true,
    });
    alpha.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    until("both held down on Beta", || {
        held(&beta.injected()) == (BTreeSet::from([SHIFT]), BTreeSet::from([1]))
    })
    .await;
}

#[when("the connection drops")]
async fn connection_drops(w: &mut LinkWorld) {
    w.sim("Beta").set_blackhole(true);
    w.sees("Alpha", "Beta", ConnectionState::Reconnecting).await;
    w.sees("Beta", "Alpha", ConnectionState::Reconnecting).await;
}

#[then("no key or button stays stuck down on any computer")]
async fn nothing_stuck(w: &mut LinkWorld) {
    let none = (BTreeSet::new(), BTreeSet::new());
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    assert_eq!(held(&alpha.injected()), none, "Alpha");
    assert_eq!(held(&beta.injected()), none, "Beta");
    // Nor does coming back press anything again.
    w.sim("Beta").set_blackhole(false);
    w.sees("Alpha", "Beta", ConnectionState::Connected).await;
    w.sees("Beta", "Alpha", ConnectionState::Connected).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(held(&alpha.injected()), none, "Alpha after reconnecting");
    assert_eq!(held(&beta.injected()), none, "Beta after reconnecting");
}

// ---- SC-008 ----------------------------------------------------------------------------

#[given("a desk computer wakes from sleep or completes a network change")]
async fn wakes_or_changes_network(w: &mut LinkWorld) {
    w.desk().await;
    w.subject = "Beta".into();
    let (beta, alpha) = (w.sim("Beta"), w.sim("Alpha"));
    beta.sleep();
    w.sees("Alpha", "Beta", ConnectionState::Reconnecting).await;
    tokio::time::sleep(ASLEEP.saturating_sub(Duration::from_secs(3))).await;
    // It wakes, and at the same moment the other computer moves to a new
    // network.
    beta.wake();
    alpha.add_interface(interface(
        "en6",
        InterfaceKind::PhysicalLan,
        [10, 20, 30, 40],
    ));
    w.since = Some(Instant::now());
}

#[when("5 seconds have passed")]
async fn five_seconds_pass(w: &mut LinkWorld) {
    let since = w.since.expect("woke");
    tokio::time::sleep(CROSS_WITHIN.saturating_sub(since.elapsed())).await;
}

#[then("the desk user can cross to it")]
async fn can_cross(w: &mut LinkWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    // The user pushes the pointer once, now: it must cross at once.
    push_toward("Alpha", &alpha);
    // The pointer crosses as soon as the input reaches the engine.
    let end = Instant::now() + CROSSING_LATENCY;
    while !alpha.is_grabbed() {
        assert!(Instant::now() < end, "the pointer did not cross");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    until("the pointer on Beta", || {
        beta.injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. }))
    })
    .await;
}
