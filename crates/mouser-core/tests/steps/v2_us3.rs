//! Step definitions for `features/v2_us3.feature`: what the desk owner finds
//! in one window, as `status` and the control API tell it.

use std::fmt;

use cucumber::{World, given, then, when};
use mouser_core::engine::EngineEvent;
use mouser_platform::{
    Capability, Limitation, Os, PermissionKind, PermissionState, PlatformInfo, SessionType,
};
use mouser_proto::control::{
    ConnectionState, Empty, EngineState, FixAction, Method, ProblemKind, Status, TrayConnection,
    UpdateResult,
};
use serde_json::{Value, json};

use crate::support::desk::{Lab, Setup};
use crate::support::release::{Host, Signer};

#[derive(World)]
#[world(init = Self::new)]
pub struct WindowWorld {
    pub lab: Lab,
    /// A second desk's worth of computers that must not meet the first's.
    pub apart: Lab,
    /// What the desk owner is looking at.
    shown: Option<Status>,
    /// The release host of the computers kept apart.
    host: Option<Host>,
    /// Computers that have no other computer to be disconnected from.
    solitary: Vec<&'static str>,
}

impl fmt::Debug for WindowWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WindowWorld({:?})", self.lab.names())
    }
}

fn setup(version: &str) -> Setup {
    Setup {
        mouser_version: version.into(),
        ..Setup::default()
    }
}

fn with_limits(version: &str, texts: &[&str]) -> Setup {
    let setup = setup(version);
    setup.platform.set_info(PlatformInfo {
        os: Os::Windows,
        version: "11".into(),
        session: SessionType::Native,
        limitations: texts
            .iter()
            .map(|text| Limitation {
                capability: Capability::Tray,
                text: (*text).into(),
            })
            .collect(),
    });
    setup
}

impl WindowWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            apart: Lab::new(),
            shown: None,
            host: None,
            solitary: Vec::new(),
        }
    }

    fn shown(&self) -> &Status {
        self.shown.as_ref().expect("the owner looked")
    }

    async fn desk(&mut self, setups: Vec<(&'static str, Setup)>) {
        let names: Vec<&str> = setups.iter().map(|(n, _)| *n).collect();
        self.lab
            .form_desk("Home office", names[0], &names[1..], setups)
            .await;
    }

    async fn look(&mut self, name: &str) {
        self.shown = Some(self.lab.node(name).status().await);
    }
}

fn problems_about(status: &Status, computer: mouser_proto::ids::ComputerId) -> usize {
    status
        .problems
        .iter()
        .filter(|p| p.computer == computer)
        .count()
}

// ---- Story 3 scenario 4 ---------------------------------------------------------------------

#[given("the Desk section is shown")]
async fn desk_section(w: &mut WindowWorld) {
    let missing = setup("0.3.0");
    missing
        .platform
        .set_permission(PermissionKind::Accessibility, PermissionState::Missing);
    w.desk(vec![
        ("Alpha", setup("0.3.0")),
        ("Beta", missing),
        ("Gamma", setup("0.2.8")),
    ])
    .await;
}

#[when("the desk owner looks at it")]
async fn looks_at_it(w: &mut WindowWorld) {
    let beta = w.lab.node("Beta").id();
    let shown = w
        .lab
        .node("Alpha")
        .wait_status("Beta's problem", |s| {
            problems_about(s, beta) == 1 && s.computers.iter().all(|c| c.displays > 0)
        })
        .await;
    w.shown = Some(shown);
}

#[then(
    "it shows the desk's name and, for each computer, its name, operating system, Mouser version, whether it is connected, which computer has the pointer, and how many problems need the owner's action, each count leading to that computer's entry in Health"
)]
async fn desk_shows_each_computer(w: &mut WindowWorld) {
    let ids: Vec<_> = ["Alpha", "Beta", "Gamma"]
        .iter()
        .map(|n| (*n, w.lab.node(n).id()))
        .collect();
    let s = w.shown();
    assert_eq!(s.desk.as_ref().expect("on a desk").name, "Home office");
    assert_eq!(s.active_computer, ids[0].1, "the pointer is on Alpha");
    let expected = [
        ("Alpha", "0.3.0", ConnectionState::SelfComputer, 0),
        ("Beta", "0.3.0", ConnectionState::Connected, 1),
        ("Gamma", "0.2.8", ConnectionState::Connected, 0),
    ];
    for (name, version, connection, problems) in expected {
        let computer = s
            .computers
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("{name} is listed"));
        assert_eq!(computer.version, version, "{name}");
        assert_eq!(computer.connection, connection, "{name}");
        assert_eq!(computer.os, mouser_proto::model::Os::Linux, "{name}");
        let id = ids.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(computer.id, id);
        // The count is of problems attributed to the computer's own entry.
        assert_eq!(problems_about(s, id), problems, "{name}");
    }
}

// ---- scenario 5 -------------------------------------------------------------------------------

#[given("a problem the desk owner can act on is raised for a computer")]
async fn actionable_problem(w: &mut WindowWorld) {
    let missing = setup("0.3.0");
    missing
        .platform
        .set_permission(PermissionKind::Accessibility, PermissionState::Missing);
    w.desk(vec![("Alpha", setup("0.3.0")), ("Beta", missing)])
        .await;
}

#[when("the owner opens Health")]
async fn owner_opens_health(w: &mut WindowWorld) {
    opens_health(w).await;
}

#[when("the desk owner opens Health")]
async fn desk_owner_opens_health(w: &mut WindowWorld) {
    opens_health(w).await;
}

async fn opens_health(w: &mut WindowWorld) {
    // Health shows what Beta told Alpha about itself.
    w.lab
        .node("Alpha")
        .wait_status("Beta's status", |s| {
            s.computers.iter().any(|c| {
                c.name == "Beta" && !(c.permissions.is_empty() && c.limitations.is_empty())
            })
        })
        .await;
    // Give a duplicate time to appear before looking at the outcome.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    w.look("Alpha").await;
}

#[then(
    "the problem appears once, in that computer's entry and no other, with its fix as one action where a fix exists"
)]
async fn appears_once(w: &mut WindowWorld) {
    let beta = w.lab.node("Beta").id();
    let s = w.shown();
    assert_eq!(s.problems.len(), 1, "{:?}", s.problems);
    let problem = &s.problems[0];
    assert_eq!(problem.kind, ProblemKind::MissingPermission);
    assert_eq!(problem.computer, beta);
    assert_eq!(
        problem.fix,
        Some(FixAction::OpenSystemSettings(
            mouser_proto::model::PermissionKind::Accessibility
        ))
    );
}

// ---- scenario 6 -------------------------------------------------------------------------------

const LIMIT: &str = "Clicking the tray icon doesn't pause Mouser here; use Pause in its menu.";

#[given("a computer has platform limitations")]
async fn has_limitations(w: &mut WindowWorld) {
    w.desk(vec![
        ("Alpha", setup("0.3.0")),
        ("Beta", with_limits("0.3.0", &[LIMIT])),
    ])
    .await;
}

#[then(
    "they are listed once, under \"What doesn't work here\" in that computer's entry, and in no other computer's entry and not as a problem notice"
)]
async fn limitations_once(w: &mut WindowWorld) {
    let shown = w
        .lab
        .node("Alpha")
        .wait_status("Beta's limitation", |s| {
            s.computers
                .iter()
                .any(|c| c.name == "Beta" && !c.limitations.is_empty())
        })
        .await;
    for name in ["Alpha", "Beta"] {
        let status = if name == "Alpha" {
            shown.clone()
        } else {
            w.lab.node("Beta").status().await
        };
        let listed: Vec<_> = status
            .computers
            .iter()
            .map(|c| (c.name.as_str(), c.limitations.clone()))
            .collect();
        for (computer, limitations) in listed {
            if computer == "Beta" {
                assert_eq!(limitations, [LIMIT], "viewed from {name}");
            } else {
                assert!(
                    limitations.iter().all(|l| l != LIMIT),
                    "{computer} viewed from {name}: {limitations:?}"
                );
            }
        }
        assert_eq!(status.problems, [], "viewed from {name}");
    }
}

// ---- scenario 8 -------------------------------------------------------------------------------

#[given("a desk computer runs a different Mouser version from this one")]
async fn different_version(w: &mut WindowWorld) {
    w.desk(vec![("Alpha", setup("0.3.0")), ("Beta", setup("0.2.8"))])
        .await;
    // Two computers whose versions cannot work together, kept apart: the
    // older one has an update to offer.
    let signer = Signer::new();
    let host = Host::start().await;
    host.publish(&signer, "0.9.0");
    let apart = |version: &str, protocol| {
        let mut setup = setup(version);
        setup.protocol_version = protocol;
        setup.release_pubkey = Some(signer.pubkey());
        setup.config.updates.release_url = host.release_url();
        setup
    };
    w.apart.add_with("Older", apart("0.1.0", 1)).await;
    let UpdateResult { available, .. } = w
        .apart
        .node("Older")
        .call(Method::UpdateCheck, json!({}))
        .await;
    assert_eq!(available.as_deref(), Some("0.9.0"));
    w.apart.add_with("Newer", apart("0.2.0", 2)).await;
    w.host = Some(host);
}

#[when("the desk owner views Desk or Health")]
async fn views_desk_or_health(w: &mut WindowWorld) {
    let shown = w
        .lab
        .node("Alpha")
        .wait_status("Beta's version", |s| {
            s.computers
                .iter()
                .any(|c| c.name == "Beta" && c.rtt_ms.is_some())
        })
        .await;
    w.shown = Some(shown);
}

#[then(
    "both versions are shown next to that computer; if the versions cannot work together, it says which computer to update and offers the update as one action where this computer can perform it"
)]
async fn both_versions(w: &mut WindowWorld) {
    let versions: Vec<_> = w
        .shown()
        .computers
        .iter()
        .map(|c| (c.name.clone(), c.version.clone()))
        .collect();
    assert!(versions.contains(&("Alpha".into(), "0.3.0".into())));
    assert!(versions.contains(&("Beta".into(), "0.2.8".into())));
    let older = w.apart.node("Older").id();
    let status = |kind: ProblemKind, s: &Status| {
        s.problems
            .iter()
            .find(|p| p.kind == kind)
            .cloned()
            .expect("the version problem")
    };
    let s = w
        .apart
        .node("Older")
        .wait_status("the version problem", |s| {
            s.problems
                .iter()
                .any(|p| p.kind == ProblemKind::IncompatibleVersion)
        })
        .await;
    let problem = status(ProblemKind::IncompatibleVersion, &s);
    assert!(
        problem.message.contains("Update Older"),
        "{}",
        problem.message
    );
    assert_eq!(problem.fix, Some(FixAction::UpdateComputer(older)));
    // The newer computer cannot perform the update, so it offers none.
    let s = w
        .apart
        .node("Newer")
        .wait_status("the version problem", |s| {
            s.problems
                .iter()
                .any(|p| p.kind == ProblemKind::IncompatibleVersion)
        })
        .await;
    let problem = status(ProblemKind::IncompatibleVersion, &s);
    assert!(
        problem.message.contains("Update Older"),
        "{}",
        problem.message
    );
    assert_eq!(problem.fix, None);
}

// ---- scenario 11 ------------------------------------------------------------------------------

#[given("a computer's displays were placed automatically")]
async fn placed_automatically(w: &mut WindowWorld) {
    w.desk(vec![
        ("Alpha", setup("0.3.0")),
        ("Beta", setup("0.3.0")),
        ("Gamma", setup("0.3.0")),
    ])
    .await;
}

fn awaiting_review(layout: &Value, computer: &str) -> bool {
    layout["displays"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["computer_name"] == computer)
        .all(|d| d["needs_review"] == json!(true))
}

#[when("the desk owner views Displays")]
async fn views_displays(w: &mut WindowWorld) {
    let (beta, gamma) = (w.lab.node("Beta").id(), w.lab.node("Gamma").id());
    let offers = w
        .lab
        .node("Alpha")
        .engine_event(
            "review offers",
            0,
            |e| matches!(e, EngineEvent::ReviewOffered { computer, .. } if *computer == gamma),
        )
        .await;
    assert!(matches!(offers, EngineEvent::ReviewOffered { name, .. } if name == "Gamma"));
    let layout: Value = w.lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    assert!(awaiting_review(&layout, "Beta"), "Beta: {layout:#}");
    assert!(awaiting_review(&layout, "Gamma"), "Gamma: {layout:#}");
    let _ = beta;
}

#[then(
    "a message names the computer, says its displays were placed automatically and should be checked against the real desk, and offers to keep the placement; the message goes away when the owner keeps or moves the placement"
)]
async fn review_goes_away(w: &mut WindowWorld) {
    let beta = w.lab.node("Beta").id();
    let layout: Value = w.lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    // Keeping Beta's placement ends its review and moves nothing.
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::LayoutKeepPlacement, json!({ "computer": beta }))
        .await;
    let kept: Value = w.lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    assert!(!awaiting_review(&kept, "Beta") && awaiting_review(&kept, "Gamma"));
    assert_eq!(kept["placements"], layout["placements"]);
    // Moving Gamma's placement ends its review.
    // Gamma goes below Alpha.
    let mut moved = kept.clone();
    let (alpha, gamma) = (
        w.lab.node("Alpha").id().to_string(),
        w.lab.node("Gamma").id().to_string(),
    );
    let at = |placements: &Value, id: &str| {
        let found = placements
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["display_id"]["computer"] == id)
            .unwrap();
        (found["x"].clone(), found["y"].as_i64().unwrap())
    };
    let (x, y) = at(&moved["placements"], &alpha);
    for placement in moved["placements"].as_array_mut().unwrap() {
        if placement["display_id"]["computer"] == gamma.as_str() {
            placement["x"] = x.clone();
            placement["y"] = json!(y + 296);
        }
    }
    let _: Value = w.lab.node("Alpha").call(Method::LayoutSet, moved).await;
    let after: Value = w.lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    assert!(!awaiting_review(&after, "Gamma"));
}

// ---- scenarios 14 and 15 -------------------------------------------------------------------------

#[given("this computer is on a desk with other computers")]
async fn on_a_desk_with_others(w: &mut WindowWorld) {
    w.desk(vec![
        ("Alpha", setup("0.3.0")),
        ("Beta", setup("0.3.0")),
        ("Gamma", setup("0.3.0")),
    ])
    .await;
}

#[when("the desk owner looks at the menu-bar or tray icon")]
async fn looks_at_the_icon(w: &mut WindowWorld) {
    // Computers with nobody to be disconnected from are read in the next step.
    if w.solitary.is_empty() {
        w.look("Alpha").await;
    }
}

#[then(
    "it shows whether this computer is connected to its desk-mates or unexpectedly disconnected from them, using a difference in shape or symbol as well as colour, alongside the active or paused state"
)]
async fn icon_shows_connection(w: &mut WindowWorld) {
    let tray = w.shown().tray_summary;
    assert_eq!(tray.connection, TrayConnection::Connected);
    assert_eq!(tray.state, EngineState::Active);
    w.lab.node("Gamma").stop().await;
    let lost = w
        .lab
        .node("Alpha")
        .wait_status("the unexpected loss", |s| {
            s.tray_summary.connection == TrayConnection::Disconnected
        })
        .await;
    assert_eq!(lost.tray_summary.state, EngineState::Active);
}

#[given(
    "this computer is the only computer on its desk, or is on no desk, or the desk owner removed the other computers or left the desk"
)]
async fn alone_in_every_way(w: &mut WindowWorld) {
    // On no desk.
    w.lab.add("Solo").await;
    // The only computer on its desk.
    w.lab.add("Lonely").await;
    let _: mouser_proto::control::Desk = w
        .lab
        .node("Lonely")
        .call(Method::DeskCreate, json!({"name": "Alone"}))
        .await;
    // The owner removed the other computer.
    w.lab
        .form_desk(
            "Pair",
            "Owner",
            &["Guest"],
            vec![("Owner", setup("0.3.0")), ("Guest", setup("0.3.0"))],
        )
        .await;
    let guest = w.lab.node("Guest").id();
    let _: Empty = w
        .lab
        .node("Owner")
        .call(Method::DeskRemoveMember, json!({ "computer_id": guest }))
        .await;
    // This computer left its desk.
    w.lab
        .form_desk(
            "Duo",
            "Stayer",
            &["Leaver"],
            vec![("Stayer", setup("0.3.0")), ("Leaver", setup("0.3.0"))],
        )
        .await;
    let _: Empty = w
        .lab
        .node("Leaver")
        .call(Method::DeskLeave, json!({}))
        .await;
    w.solitary = vec!["Solo", "Lonely", "Owner", "Leaver"];
}

#[then("it does not show the unexpectedly-disconnected state")]
async fn no_unexpected_state(w: &mut WindowWorld) {
    for name in w.solitary.clone() {
        let s = w
            .lab
            .node(name)
            .wait_status("the icon to settle", |s| {
                s.tray_summary.connection == TrayConnection::Alone
            })
            .await;
        assert_eq!(s.tray_summary.connection, TrayConnection::Alone, "{name}");
    }
}

// ---- edge cases ---------------------------------------------------------------------------------

#[given("a desk computer is mid-update to a newer version")]
async fn mid_update(w: &mut WindowWorld) {
    w.desk(vec![("Alpha", setup("0.3.0")), ("Beta", setup("0.3.0"))])
        .await;
    w.lab.node("Beta").set_version("0.3.1");
    w.lab.restart("Beta").await;
}

#[then("it shows both versions until the update completes")]
async fn both_versions_until_done(w: &mut WindowWorld) {
    let versions = |s: &Status| -> Vec<(String, String)> {
        s.computers
            .iter()
            .map(|c| (c.name.clone(), c.version.clone()))
            .collect()
    };
    let shown = w
        .lab
        .node("Alpha")
        .wait_status("Beta's new version", |s| {
            s.computers
                .iter()
                .any(|c| c.name == "Beta" && c.version == "0.3.1")
        })
        .await;
    let both = versions(&shown);
    assert!(both.contains(&("Alpha".into(), "0.3.0".into())), "{both:?}");
    assert!(both.contains(&("Beta".into(), "0.3.1".into())), "{both:?}");
    // The update completes on Alpha too.
    w.lab.node("Alpha").set_version("0.3.1");
    w.lab.restart("Alpha").await;
    let done = w
        .lab
        .node("Alpha")
        .wait_status("the update to complete", |s| {
            s.computers.len() == 2 && s.computers.iter().all(|c| c.version == "0.3.1")
        })
        .await;
    assert!(done.computers.iter().all(|c| c.version == "0.3.1"));
}

#[given("this computer is paused")]
async fn this_computer_is_paused(w: &mut WindowWorld) {
    w.desk(vec![("Alpha", setup("0.3.0")), ("Beta", setup("0.3.0"))])
        .await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActivePause, json!({}))
        .await;
}

#[when("a desk-mate goes offline")]
async fn desk_mate_goes_offline(w: &mut WindowWorld) {
    w.lab.node("Beta").stop().await;
}

#[then("the icon shows paused and the unexpectedly-disconnected state together")]
async fn paused_and_disconnected(w: &mut WindowWorld) {
    let s = w
        .lab
        .node("Alpha")
        .wait_status("the loss to show", |s| {
            s.tray_summary.connection == TrayConnection::Disconnected
        })
        .await;
    assert_eq!(s.tray_summary.state, EngineState::Paused);
    assert_eq!(s.state, EngineState::Paused);
}
