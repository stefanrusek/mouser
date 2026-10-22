//! Step definitions for `features/us8.feature`: the health panel, the one
//! action that fixes a problem, problem notifications and diagnostic logging.

use std::fmt;

use cucumber::{World, given, then, when};
use mouser_core::config;
use mouser_platform::{PermissionKind as Kind, PermissionState as State};
use mouser_proto::control::{
    ConnectionState, FixAction, LoggingSetResult, Method, ProblemKind, Status,
};
use mouser_proto::model::{PermissionKind, PermissionState};
use serde_json::{Value, json};

use crate::support::desk::{Lab, Setup};

/// The state shared by the steps of one scenario. A scenario can need more
/// than one desk (one per kind of problem), each in its own lab.
#[derive(World)]
#[world(init = Self::new)]
pub struct HealthWorld {
    pub labs: Vec<Lab>,
    /// What the scenario found out for its `Then` steps.
    seen: Vec<Value>,
    log_path: Option<String>,
}

impl fmt::Debug for HealthWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HealthWorld({} labs)", self.labs.len())
    }
}

fn schema() -> jsonschema::Validator {
    let text = include_str!("../../../../specs/001-mouser-v1/contracts/log-record.schema.json");
    jsonschema::validator_for(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

impl HealthWorld {
    fn new() -> Self {
        Self {
            labs: Vec::new(),
            seen: Vec::new(),
            log_path: None,
        }
    }

    fn lab(&mut self) -> &mut Lab {
        self.labs.last_mut().expect("a lab was started")
    }

    fn new_lab(&mut self) {
        self.labs.push(Lab::new());
    }

    /// Alpha and Beta on one desk; Beta has no Accessibility permission.
    async fn desk_where_beta_lacks_a_permission(&mut self) {
        self.new_lab();
        let beta = Setup::default();
        beta.platform
            .set_permission(Kind::Accessibility, State::Missing);
        self.lab()
            .form_desk(
                "Home office",
                "Alpha",
                &["Beta"],
                vec![("Alpha", Setup::default()), ("Beta", beta)],
            )
            .await;
    }
}

// ---- scenario 1: health panel contents ---------------------------------------------------

#[given("the desk owner opens the health panel")]
async fn opens_the_health_panel(w: &mut HealthWorld) {
    w.desk_where_beta_lacks_a_permission().await;
}

#[when("it shows")]
async fn it_shows(w: &mut HealthWorld) {
    // The health panel is the `status` result; wait for Beta's report.
    let status = w
        .lab()
        .node("Alpha")
        .wait_status("Beta's permissions", |s| {
            s.computers
                .iter()
                .any(|c| c.name == "Beta" && !c.permissions.is_empty())
        })
        .await;
    w.seen = vec![serde_json::to_value(status).unwrap()];
}

#[then(
    "it lists every computer on the desk with whether it is connected, how responsive it is, any missing permissions, and the most recent problem in plain language"
)]
async fn it_lists_every_computer(w: &mut HealthWorld) {
    let status: Status = serde_json::from_value(w.seen[0].clone()).unwrap();
    assert_eq!(status.computers.len(), 2);
    let alpha = status.computers.iter().find(|c| c.name == "Alpha").unwrap();
    assert_eq!(alpha.connection, ConnectionState::SelfComputer);
    let beta = status.computers.iter().find(|c| c.name == "Beta").unwrap();
    assert_eq!(beta.connection, ConnectionState::Connected);
    assert!(beta.rtt_ms.is_some(), "how responsive it is");
    assert_eq!(
        beta.permissions[&PermissionKind::Accessibility],
        PermissionState::Missing
    );
    let problem = status
        .problems
        .iter()
        .find(|p| p.computer == beta.id && p.kind == ProblemKind::MissingPermission)
        .expect("Beta's latest problem");
    assert!(problem.message.contains("Accessibility"), "{problem:?}");
    assert!(problem.message.contains("Mouser needs"), "plain language");
    // The same panel on Beta tells the same story about itself.
    let own = w.lab().node("Beta").status().await;
    assert!(
        own.problems
            .iter()
            .any(|p| p.kind == ProblemKind::MissingPermission)
    );
}

// ---- scenario 2: one action opens the permission screen ----------------------------------

#[given("a computer is missing a required permission")]
async fn a_computer_is_missing_a_permission(w: &mut HealthWorld) {
    w.new_lab();
    w.lab().add("Alpha").await;
    w.lab()
        .node("Alpha")
        .platform
        .set_permission(Kind::InputMonitoring, State::Missing);
}

#[when("the owner views it in the health panel")]
async fn views_it(w: &mut HealthWorld) {
    let status = w
        .lab()
        .node("Alpha")
        .wait_status("the missing permission", |s| {
            s.problems
                .iter()
                .any(|p| p.kind == ProblemKind::MissingPermission)
        })
        .await;
    w.seen = vec![serde_json::to_value(status).unwrap()];
}

#[then("a single action opens the system screen where that permission is granted")]
async fn a_single_action_opens_the_screen(w: &mut HealthWorld) {
    let status: Status = serde_json::from_value(w.seen[0].clone()).unwrap();
    let problem = status
        .problems
        .iter()
        .find(|p| p.kind == ProblemKind::MissingPermission)
        .unwrap();
    assert_eq!(
        problem.fix,
        Some(FixAction::OpenSystemSettings(
            PermissionKind::InputMonitoring
        ))
    );
    let node = w.lab().node("Alpha");
    assert!(node.platform.opened_permission_settings().is_empty());
    node.control().fix(&problem.problem_id).unwrap();
    assert_eq!(
        node.platform.opened_permission_settings(),
        [Kind::InputMonitoring]
    );
}

// ---- scenario 3: notification for unfixable problems -------------------------------------

#[given(
    "a problem occurs that Mouser could not fix by itself (for example a refused computer, an incompatible version, or a missing permission)"
)]
async fn a_problem_occurs(w: &mut HealthWorld) {
    // A missing permission, on a computer that is running already.
    w.new_lab();
    w.lab().add("Alpha").await;
    // A refused computer: an allowlist that does not include the joiner.
    w.new_lab();
    let mut owner = Setup::default();
    owner.config.advanced.allowlist = vec!["10.0.0.0/8".into()];
    w.lab().add_with("Gamma", owner).await;
    w.lab().add("Delta").await;
    // An incompatible version.
    w.new_lab();
    w.lab().add("Epsilon").await;
}

#[when("it occurs")]
async fn it_occurs(w: &mut HealthWorld) {
    w.labs[0]
        .node("Alpha")
        .platform
        .set_permission(Kind::Accessibility, State::Missing);
    let lab = &mut w.labs[1];
    let desk: mouser_proto::control::Desk = lab
        .node("Gamma")
        .call(Method::DeskCreate, json!({"name": "Home office"}))
        .await;
    let _: mouser_proto::control::JoinPending = lab
        .node("Delta")
        .call(Method::DeskJoin, json!({"desk_id": desk.desk_id}))
        .await;
    let newer = Setup {
        mouser_version: "0.2.0".into(),
        protocol_version: 2,
        ..Setup::default()
    };
    w.labs[2].add_with("Zeta", newer).await;
}

fn assert_notice(note: &Value, kind: ProblemKind, fix: Option<&str>) {
    assert_eq!(note["kind"], json!(kind));
    assert!(!note["message"].as_str().unwrap().is_empty());
    assert_eq!(note["fix"].as_str(), fix, "{note}");
}

#[then("a notification states the problem and, where a fix exists, offers it as one action")]
async fn a_notification_states_the_problem(w: &mut HealthWorld) {
    let missing = w.labs[0]
        .node("Alpha")
        .notification(
            "the permission notice",
            mouser_proto::control::Topic::Problem,
            |p| p["kind"] == json!(ProblemKind::MissingPermission),
        )
        .await;
    assert_notice(
        &missing,
        ProblemKind::MissingPermission,
        Some("open_system_settings:accessibility"),
    );
    let refused = w.labs[1]
        .node("Gamma")
        .notification("the refusal", mouser_proto::control::Topic::Problem, |p| {
            p["kind"] == json!(ProblemKind::JoinRefused)
        })
        .await;
    assert_notice(&refused, ProblemKind::JoinRefused, None);
    let version = w.labs[2]
        .node("Epsilon")
        .notification(
            "the version notice",
            mouser_proto::control::Topic::Problem,
            |p| p["kind"] == json!(ProblemKind::IncompatibleVersion),
        )
        .await;
    assert_notice(
        &version,
        ProblemKind::IncompatibleVersion,
        Some("open_update"),
    );
}

// ---- scenario 4: diagnostic logging checkbox ---------------------------------------------

#[given("the owner turns on diagnostic logging with one checkbox")]
async fn turns_on_logging(w: &mut HealthWorld) {
    w.new_lab();
    w.lab().add("Alpha").await;
    w.lab().add("Beta").await;
    let on: LoggingSetResult = w
        .lab()
        .node("Alpha")
        .call(Method::LoggingSet, json!({"enabled": true}))
        .await;
    w.log_path = Some(on.path);
}

#[when("Mouser runs afterwards")]
async fn mouser_runs_afterwards(w: &mut HealthWorld) {
    // Things happen: a desk forms.
    let _: mouser_proto::control::Desk = w
        .lab()
        .node("Alpha")
        .call(Method::DeskCreate, json!({"name": "Home office"}))
        .await;
    let target = w.lab().node("Alpha").sim.clone();
    let address = w.lab().node("Beta").sim.route_to(&target).to_string();
    let _: mouser_proto::control::JoinPending = w
        .lab()
        .node("Beta")
        .call(Method::DeskJoin, json!({ "address": address }))
        .await;
    let value = w
        .lab()
        .node("Alpha")
        .notification(
            "a join request",
            mouser_proto::control::Topic::JoinRequest,
            |_| true,
        )
        .await;
    let request: mouser_proto::control::PendingJoin = serde_json::from_value(value).unwrap();
    let _: mouser_proto::control::Empty = w
        .lab()
        .node("Alpha")
        .call(
            Method::JoinDecide,
            json!({"request_id": request.request_id, "approve": true}),
        )
        .await;
    w.lab()
        .node("Alpha")
        .wait_status("Beta connected", |s| {
            s.computers
                .iter()
                .any(|c| c.connection == ConnectionState::Connected)
        })
        .await;
}

#[then(
    "diagnostic logs are written to a file whose location the owner can open from the same place"
)]
async fn logs_are_written(w: &mut HealthWorld) {
    let path = w.log_path.clone().unwrap();
    let status = w.lab().node("Alpha").status().await;
    let logging = status.logging.expect("the checkbox's state");
    assert!(logging.enabled);
    assert_eq!(
        logging.path, path,
        "the location is shown with the checkbox"
    );
    let dir = w.lab().node("Alpha").dir.path().to_path_buf();
    assert!(std::path::Path::new(&path).starts_with(&dir));
    let text = std::fs::read_to_string(&path).expect("the log file exists");
    let schema = schema();
    let mut events = Vec::new();
    for line in text.lines() {
        let record: Value = serde_json::from_str(line).expect("one JSON record per line");
        assert!(schema.is_valid(&record), "{line}");
        events.push(record["event"].as_str().unwrap().to_string());
    }
    assert!(
        events.contains(&"logging.enabled".to_string()),
        "{events:?}"
    );
    assert!(events.contains(&"peer.connected".to_string()), "{events:?}");
    // It is the config's setting, so it is still on after a restart.
    assert!(config::load_or_create(&dir).diagnostics.logging);
}
