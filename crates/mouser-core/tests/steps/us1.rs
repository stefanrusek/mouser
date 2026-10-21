//! Step definitions for `features/us1.feature`: putting computers on a desk.

use std::fmt;
use std::time::Duration;

use cucumber::{World, given, then, when};
use mouser_core::control::status::tray_summary;
use mouser_platform::{PermissionKind as PlatformKind, PermissionState as PlatformState, Platform};
use mouser_proto::control::{
    ConnectionState, Desk, DiscoveredDesk, Empty, EngineState, FixAction, JoinPending, Method,
    PendingJoin, ProblemKind, Status, Topic,
};
use mouser_proto::ids::DeskId;
use mouser_proto::model::{PermissionKind, PermissionState};
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use crate::support::desk::{Lab, Setup};

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct DeskWorld {
    pub lab: Lab,
    status: Option<Status>,
    desk: Option<Desk>,
    request: Option<PendingJoin>,
}

impl fmt::Debug for DeskWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeskWorld({:?})", self.lab.names())
    }
}

impl DeskWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            status: None,
            desk: None,
            request: None,
        }
    }

    async fn create_desk(&mut self, node: &str, name: &str) -> Desk {
        self.lab
            .node(node)
            .call(Method::DeskCreate, json!({ "name": name }))
            .await
    }

    /// The prompt shown on `member` for a request from `joiner`.
    async fn prompt(&mut self, member: &str, joiner: &str) -> PendingJoin {
        let value = self
            .lab
            .node(member)
            .notification(
                &format!("a join request from {joiner}"),
                Topic::JoinRequest,
                |p| p["computer"]["name"] == joiner,
            )
            .await;
        serde_json::from_value(value).unwrap()
    }

    async fn decide(&mut self, member: &str, request: &PendingJoin, approve: bool) {
        let _: Empty = self
            .lab
            .node(member)
            .call(
                Method::JoinDecide,
                json!({"request_id": request.request_id, "approve": approve}),
            )
            .await;
    }

    /// Wait until `name` is connected to `count` other computers.
    async fn connected(&mut self, name: &str, count: usize) {
        self.lab
            .node(name)
            .wait_status(&format!("{count} connected computers"), |s| {
                s.computers
                    .iter()
                    .filter(|c| c.connection == ConnectionState::Connected)
                    .count()
                    == count
            })
            .await;
    }

    /// `joiner` joins `member`'s desk by its network address, `member`'s owner
    /// clicks Allow.
    async fn join_by_address(&mut self, member: &str, joiner: &str) {
        let target = self.lab.node(member).sim.clone();
        let address = self.lab.node(joiner).sim.route_to(&target).to_string();
        let _: JoinPending = self
            .lab
            .node(joiner)
            .call(Method::DeskJoin, json!({ "address": address }))
            .await;
        let request = self.prompt(member, joiner).await;
        self.decide(member, &request, true).await;
    }

    /// Put `joiners` on the desk `creator` creates, fully connected.
    async fn desk_of(&mut self, creator: &str, joiners: &[&str], name: &str) -> Desk {
        let desk = self.create_desk(creator, name).await;
        for joiner in joiners {
            self.join_by_address(creator, joiner).await;
        }
        for member in std::iter::once(&creator).chain(joiners) {
            self.connected(member, joiners.len()).await;
        }
        desk
    }

    async fn member_names(&mut self, node: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .lab
            .node(node)
            .status()
            .await
            .computers
            .into_iter()
            .map(|c| c.name)
            .collect();
        names.sort();
        names
    }
}

// ---- scenario 1: first-run permissions ---------------------------------------

#[given("Mouser is launched for the first time")]
async fn first_launch(w: &mut DeskWorld) {
    let platform = VirtualPlatform::new();
    platform.set_permission(PlatformKind::Accessibility, PlatformState::Missing);
    platform.set_permission(PlatformKind::InputMonitoring, PlatformState::Granted);
    platform.set_permission(PlatformKind::ScreenRecording, PlatformState::Missing);
    platform.set_permission(PlatformKind::FullDiskAccess, PlatformState::NotApplicable);
    w.lab
        .add_with(
            "Newcomer",
            Setup {
                platform,
                ..Setup::default()
            },
        )
        .await;
}

#[when("the first-run flow starts")]
async fn first_run_starts(w: &mut DeskWorld) {
    w.status = Some(w.lab.node("Newcomer").status().await);
}

#[then(
    "it walks the owner through each operating-system permission Mouser needs, showing which are granted and opening the right system screen for each one still missing"
)]
async fn permissions_walkthrough(w: &mut DeskWorld) {
    let status = w.status.take().expect("status read");
    let me = &status.computers[0];
    assert_eq!(
        me.permissions[&PermissionKind::Accessibility],
        PermissionState::Missing
    );
    assert_eq!(
        me.permissions[&PermissionKind::InputMonitoring],
        PermissionState::Granted
    );
    assert_eq!(
        me.permissions[&PermissionKind::ScreenRecording],
        PermissionState::Missing
    );
    assert_eq!(
        me.permissions[&PermissionKind::FullDiskAccess],
        PermissionState::NotApplicable
    );

    // One problem per missing permission, each with the fix that opens its screen.
    let mut fixes: Vec<FixAction> = status
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::MissingPermission)
        .map(|p| p.fix.expect("a missing permission has a fix"))
        .collect();
    fixes.sort_by_key(|f| f.to_string());
    assert_eq!(
        fixes,
        [
            FixAction::OpenSystemSettings(PermissionKind::Accessibility),
            FixAction::OpenSystemSettings(PermissionKind::ScreenRecording),
        ]
    );
    // The UI acts on each fix by opening that permission's system screen.
    let platform = w.lab.node("Newcomer").platform.clone();
    for fix in fixes {
        let FixAction::OpenSystemSettings(kind) = fix else {
            panic!("only settings fixes expected");
        };
        let kind = match kind {
            PermissionKind::Accessibility => PlatformKind::Accessibility,
            PermissionKind::ScreenRecording => PlatformKind::ScreenRecording,
            other => panic!("unexpected {other:?}"),
        };
        platform.open_permission_settings(kind).unwrap();
    }
    assert_eq!(
        platform.opened_permission_settings(),
        [PlatformKind::Accessibility, PlatformKind::ScreenRecording]
    );
}

// ---- scenario 2: join with one click ----------------------------------------

#[given(regex = r#"^another computer on the network belongs to the desk "([^"]*)"$"#)]
async fn another_computer_has_a_desk(w: &mut DeskWorld, name: String) {
    w.lab.add("Alpha").await;
    w.desk = Some(w.create_desk("Alpha", &name).await);
}

#[when(
    regex = r#"^the owner reaches the desk step on a new computer, chooses "([^"]*)", and clicks Allow on the prompt shown on a computer already on that desk$"#
)]
async fn choose_and_allow(w: &mut DeskWorld, name: String) {
    w.lab.add("Beta").await;
    let offered: Vec<DiscoveredDesk> = w
        .lab
        .node("Beta")
        .call(Method::DesksDiscovered, json!({}))
        .await;
    let chosen = offered
        .iter()
        .find(|d| d.name == name)
        .expect("the desk is offered");
    let _: JoinPending = w
        .lab
        .node("Beta")
        .call(Method::DeskJoin, json!({"desk_id": chosen.desk_id}))
        .await;
    let request = w.prompt("Alpha", "Beta").await;
    w.decide("Alpha", &request, true).await;
}

#[then("the new computer is a member, with no code, password, or address typed")]
async fn new_computer_is_a_member(w: &mut DeskWorld) {
    let desk_id = w.desk.as_ref().unwrap().desk_id;
    for name in ["Alpha", "Beta"] {
        let status = w
            .lab
            .node(name)
            .wait_status("both computers on the desk", |s| {
                s.computers.len() == 2
                    && s.computers
                        .iter()
                        .all(|c| c.connection != ConnectionState::Offline)
            })
            .await;
        assert_eq!(status.desk.as_ref().unwrap().id, desk_id);
        assert_eq!(status.desk.unwrap().pending_join, None);
        let mut names: Vec<_> = status.computers.iter().map(|c| c.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["Alpha", "Beta"], "{name} lists both computers");
    }
}

// ---- scenario 3: decline, create ----------------------------------------------

#[given("a join request is pending")]
async fn a_request_is_pending(w: &mut DeskWorld) {
    w.lab.add("Alpha").await;
    let desk = w.create_desk("Alpha", "Home office").await;
    w.lab.add("Beta").await;
    let _: JoinPending = w
        .lab
        .node("Beta")
        .call(Method::DeskJoin, json!({"desk_id": desk.desk_id}))
        .await;
    w.request = Some(w.prompt("Alpha", "Beta").await);
}

#[when("the owner declines it on a desk computer")]
async fn owner_declines(w: &mut DeskWorld) {
    let request = w.request.take().unwrap();
    w.decide("Alpha", &request, false).await;
}

#[then("the requesting computer is told it was not admitted and is not a member")]
async fn requester_is_told(w: &mut DeskWorld) {
    let problem = w
        .lab
        .node("Beta")
        .notification("a not-admitted notice", Topic::Problem, |p| {
            p["kind"] == json!(ProblemKind::JoinRefused)
        })
        .await;
    assert!(
        problem["message"]
            .as_str()
            .unwrap()
            .contains("not admitted"),
        "{problem}"
    );
    let status = w
        .lab
        .node("Beta")
        .wait_status("Beta to be unjoined", |s| s.desk.is_none())
        .await;
    assert_eq!(status.computers.len(), 1);
    assert_eq!(
        w.lab.node("Alpha").status().await.computers.len(),
        1,
        "Alpha is still alone"
    );
}

#[given("no desk is found, or the owner prefers a new one")]
async fn no_desk_found(w: &mut DeskWorld) {
    w.lab.add("Alpha").await;
    w.lab.add("Beta").await;
    let offered: Vec<DiscoveredDesk> = w
        .lab
        .node("Alpha")
        .call(Method::DesksDiscovered, json!({}))
        .await;
    assert!(offered.is_empty(), "no desk on the network yet");
}

#[when("they choose to create a desk and give it a name")]
async fn they_create_a_desk(w: &mut DeskWorld) {
    w.desk = Some(w.create_desk("Alpha", "Home office").await);
}

#[then(
    "the desk exists with this computer as its only member and is offered to other computers on the network"
)]
async fn desk_exists_and_is_offered(w: &mut DeskWorld) {
    let desk = w.desk.take().unwrap();
    let status = w.lab.node("Alpha").status().await;
    assert_eq!(status.desk.as_ref().unwrap().name, "Home office");
    assert_eq!(status.desk.unwrap().id, desk.desk_id);
    assert_eq!(status.computers.len(), 1);
    assert_eq!(
        status.computers[0].connection,
        ConnectionState::SelfComputer
    );
    assert_eq!(desk.state.members.len(), 1);
    let offered = wait_for_desks(w, "Beta", 1).await;
    assert_eq!(offered[0].desk_id, desk.desk_id);
    assert_eq!(offered[0].members, ["Alpha"]);
}

async fn wait_for_desks(w: &mut DeskWorld, node: &str, count: usize) -> Vec<DiscoveredDesk> {
    for _ in 0..800 {
        let offered: Vec<DiscoveredDesk> = w
            .lab
            .node(node)
            .call(Method::DesksDiscovered, json!({}))
            .await;
        if offered.len() == count {
            return offered;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{node} never saw {count} desks");
}

// ---- scenario 4: two desks are isolated ------------------------------------------

#[given(regex = r#"^two desks, "([^"]*)" and "([^"]*)", on one network$"#)]
async fn two_desks(w: &mut DeskWorld, first: String, second: String) {
    w.lab.add("Home1").await;
    w.lab.add("Home2").await;
    w.lab.add("Lab1").await;
    w.lab.add("Lab2").await;
    w.desk_of("Home1", &["Home2"], &first).await;
    w.desk_of("Lab1", &["Lab2"], &second).await;
}

#[when(regex = r#"^a computer on "([^"]*)" is in use$"#)]
async fn lab_computer_in_use(w: &mut DeskWorld, _desk: String) {
    use mouser_platform::{ClipRep, LocalInput};
    let lab1 = w.lab.node("Lab1").platform.clone();
    lab1.send_input(LocalInput::Motion {
        dx: 5.0,
        dy: 0.0,
        abs: (100.0, 100.0),
    });
    lab1.send_input(LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    });
    lab1.set_local_clipboard(vec![ClipRep::Text("secret".into())]);
    // Long enough for several discovery rounds in which the desks see each other.
    tokio::time::sleep(Duration::from_millis(1500)).await;
}

#[then(
    regex = r#"^computers on "([^"]*)" receive none of its pointer movement, keystrokes, clipboard contents, or files, and neither desk lists the other's computers as members$"#
)]
async fn nothing_crosses(w: &mut DeskWorld, _desk: String) {
    for home in ["Home1", "Home2"] {
        let platform = w.lab.node(home).platform.clone();
        assert!(platform.injected().is_empty(), "{home} received input");
        assert!(
            platform.clipboard_contents().is_empty(),
            "{home} received clipboard contents"
        );
        assert!(
            platform.incoming_drags().is_empty(),
            "{home} received files"
        );
    }
    assert_eq!(w.member_names("Home1").await, ["Home1", "Home2"]);
    assert_eq!(w.member_names("Home2").await, ["Home1", "Home2"]);
    assert_eq!(w.member_names("Lab1").await, ["Lab1", "Lab2"]);
    assert_eq!(w.member_names("Lab2").await, ["Lab1", "Lab2"]);
    // The desks never even connected to each other.
    for home in ["Home1", "Home2"] {
        let status = w.lab.node(home).status().await;
        assert_eq!(
            status
                .computers
                .iter()
                .filter(|c| c.connection == ConnectionState::Connected)
                .count(),
            1
        );
    }
}

// ---- scenario 5: move or remove ----------------------------------------------------

#[given("a computer is on a desk")]
async fn a_computer_is_on_a_desk(w: &mut DeskWorld) {
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        w.lab.add(name).await;
    }
    w.desk_of("Alpha", &["Beta", "Gamma"], "Home office").await;
    w.desk = Some(w.create_desk("Delta", "Lab bench").await);
}

#[when("the owner moves it to another desk or removes it from its desk")]
async fn move_or_remove(w: &mut DeskWorld) {
    // Beta moves to Lab bench, which Delta approves.
    let lab_bench = w.desk.take().unwrap();
    let _: JoinPending = w
        .lab
        .node("Beta")
        .call(Method::DeskMove, json!({"desk_id": lab_bench.desk_id}))
        .await;
    let request = w.prompt("Delta", "Beta").await;
    w.decide("Delta", &request, true).await;
    w.connected("Delta", 1).await;
    // Once Alpha and Gamma have settled on the two-member desk, Alpha removes Gamma.
    for name in ["Alpha", "Gamma"] {
        w.lab
            .node(name)
            .wait_status("Beta to be gone", |s| {
                s.computers.len() == 2
                    && s.computers
                        .iter()
                        .all(|c| c.name != "Beta" && c.connection != ConnectionState::Offline)
            })
            .await;
    }
    let gamma = w.lab.node("Gamma").id();
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::DeskRemoveMember, json!({ "computer_id": gamma }))
        .await;
}

#[then(regex = r"^it appears only in the new desk's member list \(or none\) on every computer$")]
async fn only_in_the_new_list(w: &mut DeskWorld) {
    let expect = [
        ("Alpha", vec!["Alpha"]),
        ("Beta", vec!["Beta", "Delta"]),
        ("Gamma", vec!["Gamma"]),
        ("Delta", vec!["Beta", "Delta"]),
    ];
    for (name, members) in expect {
        let want: Vec<String> = members.iter().map(ToString::to_string).collect();
        let mut seen = Vec::new();
        for _ in 0..800 {
            seen = w.member_names(name).await;
            if seen == want {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(seen, want, "member list on {name}");
    }
    assert_eq!(
        w.lab.node("Gamma").status().await.desk,
        None,
        "the removed computer is on no desk"
    );
}

// ---- scenario 6: add by address ------------------------------------------------------

#[given("a computer cannot be found automatically")]
async fn cannot_be_found(w: &mut DeskWorld) {
    w.lab.add("Alpha").await;
    w.desk = Some(w.create_desk("Alpha", "Home office").await);
    w.lab
        .add_with(
            "Beta",
            Setup {
                blind: true,
                ..Setup::default()
            },
        )
        .await;
    let offered: Vec<DiscoveredDesk> = w
        .lab
        .node("Beta")
        .call(Method::DesksDiscovered, json!({}))
        .await;
    assert!(offered.is_empty(), "Beta finds nothing by itself");
}

#[when("the owner adds it by network address")]
async fn add_by_address(w: &mut DeskWorld) {
    w.join_by_address("Alpha", "Beta").await;
}

#[then("it can join the desk like a discovered computer")]
async fn joined_like_discovered(w: &mut DeskWorld) {
    let desk_id: DeskId = w.desk.take().unwrap().desk_id;
    w.connected("Alpha", 1).await;
    w.connected("Beta", 1).await;
    let status = w.lab.node("Beta").status().await;
    assert_eq!(status.desk.unwrap().id, desk_id);
    assert_eq!(w.member_names("Alpha").await, ["Alpha", "Beta"]);
}

// ---- scenario 7: allowlist --------------------------------------------------------------

#[given("the owner has turned on the allowlist")]
async fn allowlist_on(w: &mut DeskWorld) {
    let mut config = mouser_proto::config::Config::default();
    config.advanced.allowlist = vec!["10.0.0.0/8".into()];
    w.lab
        .add_with(
            "Alpha",
            Setup {
                config,
                ..Setup::default()
            },
        )
        .await;
    w.desk = Some(w.create_desk("Alpha", "Home office").await);
}

#[when("a computer whose address is not on the list tries to join")]
async fn outsider_tries(w: &mut DeskWorld) {
    w.lab.add("Beta").await;
    let desk_id = w.desk.as_ref().unwrap().desk_id;
    let _: JoinPending = w
        .lab
        .node("Beta")
        .call(Method::DeskJoin, json!({ "desk_id": desk_id }))
        .await;
}

#[then("it is not admitted and the owner sees which computer was refused")]
async fn outsider_refused(w: &mut DeskWorld) {
    let beta = w.lab.node("Beta").id().to_string();
    let refused = w
        .lab
        .node("Alpha")
        .notification("a refusal", Topic::Problem, |p| {
            p["kind"] == json!(ProblemKind::JoinRefused)
        })
        .await;
    assert!(
        refused["message"].as_str().unwrap().contains("Beta"),
        "{refused}"
    );
    assert_eq!(refused["computer"], json!(beta));
    let told = w
        .lab
        .node("Beta")
        .notification("a not-admitted notice", Topic::Problem, |p| {
            p["kind"] == json!(ProblemKind::JoinRefused)
        })
        .await;
    assert!(
        told["message"].as_str().unwrap().contains("not admitted"),
        "{told}"
    );
    w.lab
        .node("Beta")
        .wait_status("Beta to be unjoined", |s| s.desk.is_none())
        .await;
    let pending: Vec<PendingJoin> = w
        .lab
        .node("Alpha")
        .call(Method::JoinPending, json!({}))
        .await;
    assert!(pending.is_empty(), "no prompt was shown");
    assert!(
        w.lab
            .node("Alpha")
            .events
            .all(Topic::JoinRequest)
            .is_empty(),
        "no prompt notification either"
    );
    assert_eq!(w.lab.node("Alpha").status().await.computers.len(), 1);
}

// ---- scenario 8: incompatible versions ------------------------------------------------------

#[given("two computers run Mouser versions that cannot work together")]
async fn incompatible_pair(w: &mut DeskWorld) {
    w.lab
        .add_with(
            "Alpha",
            Setup {
                protocol_version: 1,
                mouser_version: "0.1.0".into(),
                ..Setup::default()
            },
        )
        .await;
    w.lab
        .add_with(
            "Beta",
            Setup {
                protocol_version: 2,
                mouser_version: "0.2.0".into(),
                ..Setup::default()
            },
        )
        .await;
}

#[when("they find each other")]
async fn they_find_each_other(w: &mut DeskWorld) {
    for name in ["Alpha", "Beta"] {
        w.lab
            .node(name)
            .wait_status("the version problem", |s| {
                s.problems
                    .iter()
                    .any(|p| p.kind == ProblemKind::IncompatibleVersion)
            })
            .await;
    }
}

#[then(
    "each shows a plain-language message naming the other computer, both versions, and which one to update"
)]
async fn each_shows_a_message(w: &mut DeskWorld) {
    for (name, other) in [("Alpha", "Beta"), ("Beta", "Alpha")] {
        // What the computer shows is its status: a computer that finds the
        // other while it is still starting up raises the problem before any
        // client could have subscribed, and status is what such a client
        // reads first.
        let problem = w
            .lab
            .node(name)
            .status()
            .await
            .problems
            .into_iter()
            .find(|p| p.kind == ProblemKind::IncompatibleVersion)
            .expect("a version problem");
        let text = &problem.message;
        assert!(text.contains(other), "{name}: {text}");
        assert!(
            text.contains("0.1.0") && text.contains("0.2.0"),
            "{name}: {text}"
        );
        assert!(
            text.contains("Update Alpha"),
            "{name}: the older one is named: {text}"
        );
    }
    // Only the older computer is offered the update.
    let alpha = w.lab.node("Alpha").status().await;
    let version_fix = |status: &Status| {
        status
            .problems
            .iter()
            .find(|p| p.kind == ProblemKind::IncompatibleVersion)
            .expect("a version problem")
            .fix
    };
    assert_eq!(version_fix(&alpha), Some(FixAction::OpenUpdate));
    let beta = w.lab.node("Beta").status().await;
    assert_eq!(version_fix(&beta), None);
}

// ---- scenario 9: start at login ---------------------------------------------------------------

#[given("setup is complete")]
async fn setup_is_complete(w: &mut DeskWorld) {
    w.lab.add("Alpha").await;
    w.lab.add("Beta").await;
    w.desk = Some(w.desk_of("Alpha", &["Beta"], "Home office").await);
    assert!(
        w.lab.node("Beta").platform.is_autostart_enabled(),
        "start at login was registered"
    );
}

#[when("the computer restarts and the owner logs in")]
async fn computer_restarts(w: &mut DeskWorld) {
    w.lab.restart("Beta").await;
}

#[then("Mouser is running and the computer is back on its desk without any action")]
async fn back_on_its_desk(w: &mut DeskWorld) {
    let desk_id = w.desk.take().unwrap().desk_id;
    let status = w
        .lab
        .node("Beta")
        .wait_status("Beta to reconnect on its own", |s| {
            s.computers
                .iter()
                .filter(|c| c.connection == ConnectionState::Connected)
                .count()
                == 1
        })
        .await;
    assert_eq!(status.desk.unwrap().id, desk_id);
    w.connected("Alpha", 1).await;
    assert!(w.lab.node("Beta").platform.is_autostart_enabled());
}

// ---- scenario 10: tray icon ------------------------------------------------------------------

#[given("Mouser is running")]
async fn mouser_is_running(w: &mut DeskWorld) {
    w.lab.add("Alpha").await;
    let platform = VirtualPlatform::new();
    let mut displays = platform.displays().unwrap();
    let mut second = displays[0].clone();
    second.key = "virtual-1".into();
    second.origin = (1920.0, 0.0);
    displays.push(second);
    platform.set_displays(displays);
    w.lab
        .add_with(
            "Beta",
            Setup {
                platform,
                ..Setup::default()
            },
        )
        .await;
    w.desk_of("Alpha", &["Beta"], "Home office").await;
}

#[when("the owner looks at the menu bar or system tray")]
async fn owner_looks_at_the_tray(w: &mut DeskWorld) {
    w.status = Some(
        w.lab
            .node("Alpha")
            .wait_status("Beta's displays", |s| {
                s.computers.iter().map(|c| c.displays).sum::<u32>() == 3
            })
            .await,
    );
}

#[then(
    "an icon matching the system's light or dark appearance shows whether Mouser is active or paused and how many computers and displays are on the desk"
)]
async fn tray_shows_state_and_counts(w: &mut DeskWorld) {
    // The core supplies the state and the counts; choosing the light or dark
    // icon variant is the app's job.
    let tray = tray_summary(&w.status.take().unwrap());
    assert_eq!(
        (tray.state, tray.computers, tray.displays),
        (EngineState::Active, 2, 3)
    );
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActivePause, json!({}))
        .await;
    let paused = tray_summary(&w.lab.node("Alpha").status().await);
    assert_eq!(
        (paused.state, paused.computers, paused.displays),
        (EngineState::Paused, 2, 3)
    );
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActiveResume, json!({}))
        .await;
    assert_eq!(
        tray_summary(&w.lab.node("Alpha").status().await).state,
        EngineState::Active
    );
}

// ---- edge case: two desks with the same name -----------------------------------------------------

#[given("two computers on the network")]
async fn two_computers(w: &mut DeskWorld) {
    for name in ["Alpha", "Beta", "Gamma"] {
        w.lab.add(name).await;
    }
}

#[when("they try to create desks with the same name")]
async fn same_name_desks(w: &mut DeskWorld) {
    w.create_desk("Alpha", "Home office").await;
    w.create_desk("Beta", "Home office").await;
}

#[then("both desks are offered, distinguished by their member computers' names")]
async fn both_offered(w: &mut DeskWorld) {
    // Gamma, an unjoined computer on the same network, is offered both.
    let offered = wait_for_desks(w, "Gamma", 2).await;
    assert!(offered.iter().all(|d| d.name == "Home office"));
    assert_ne!(offered[0].desk_id, offered[1].desk_id);
    let mut members: Vec<_> = offered.iter().map(|d| d.members.clone()).collect();
    members.sort();
    assert_eq!(
        members,
        [vec!["Alpha".to_string()], vec!["Beta".to_string()]]
    );
}
