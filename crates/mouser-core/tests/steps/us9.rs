//! Step definitions for `features/us9.feature`: an agent works through the
//! control socket (what the command-line tool speaks), the configuration
//! file and the diagnostic log, with no GUI.

use std::fmt;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_platform::{PermissionKind as Kind, PermissionState as State};
use mouser_proto::control::{
    ConnectionState, LoggingSetResult, Method, ProblemKind, Status, Topic,
};
use mouser_proto::model::{PermissionKind, PermissionState};
use serde_json::{Value, json};

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct AgentWorld {
    pub lab: Lab,
    /// What the scenario found out for its `Then` steps.
    seen: Vec<Value>,
    /// Every setting of the GUI with the value the agent gives it.
    changes: Vec<(&'static str, Value)>,
    log_path: Option<String>,
}

impl fmt::Debug for AgentWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentWorld({:?})", self.lab.names())
    }
}

/// Every key of contracts/config.md (all settings of the GUI), each with a
/// value other than its default; `second` is a different one again.
fn settings(second: bool) -> Vec<(&'static str, Value)> {
    let pick = |a: Value, b: Value| if second { b } else { a };
    vec![
        ("general.start_at_login", pick(json!(false), json!(true))),
        ("general.notifications", pick(json!(false), json!(true))),
        (
            "general.computer_name",
            pick(json!("Desk Mac"), json!("Renamed by the agent")),
        ),
        (
            "hotkeys.enable_disable",
            pick(json!("Ctrl+Alt+E"), json!("Ctrl+Alt+D")),
        ),
        (
            "hotkeys.quickjump",
            pick(json!("Ctrl+Alt+Q"), json!("Ctrl+Alt+K")),
        ),
        (
            "hotkeys.targeted_paste",
            pick(json!("Ctrl+Alt+P"), json!("Ctrl+Alt+O")),
        ),
        ("hotkeys.panic", pick(json!("F5*3"), json!("F6*2"))),
        (
            "mouse.buttons_4_5",
            pick(json!("back_forward"), json!("passthrough")),
        ),
        ("crossing.block_fullscreen", pick(json!(false), json!(true))),
        (
            "crossing.equalize_dimensions",
            pick(json!(true), json!(false)),
        ),
        ("diagnostics.logging", pick(json!(true), json!(false))),
        ("diagnostics.max_log_mb", pick(json!(10), json!(20))),
        ("updates.automatic", pick(json!(false), json!(true))),
        (
            "updates.release_url",
            pick(
                json!("http://localhost/one.json"),
                json!("http://localhost/two.json"),
            ),
        ),
        ("advanced.port", pick(json!(48000), json!(48001))),
        (
            "advanced.allowlist",
            pick(json!(["127.0.0.0/8", "fd00::/8"]), json!(["127.0.0.0/8"])),
        ),
    ]
}

fn schema() -> jsonschema::Validator {
    let text = include_str!("../../../../specs/001-mouser-v1/contracts/log-record.schema.json");
    jsonschema::validator_for(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

fn status_schema() -> jsonschema::Validator {
    let text = include_str!("../../../mouser-proto/schemas/status.schema.json");
    jsonschema::validator_for(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
}

impl AgentWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            seen: Vec::new(),
            changes: Vec::new(),
            log_path: None,
        }
    }

    /// Alpha and Beta on one desk.
    async fn desk(&mut self, beta: Setup) {
        self.lab
            .form_desk(
                "Home office",
                "Alpha",
                &["Beta"],
                vec![("Alpha", Setup::default()), ("Beta", beta)],
            )
            .await;
    }

    /// `config.get` of `key` on `name`, as the command-line tool asks.
    async fn get(&mut self, name: &str, key: &str) -> Value {
        self.lab
            .node(name)
            .call(Method::ConfigGet, json!({ "key": key }))
            .await
    }

    /// Wait until `key` reads `value` on `name`.
    async fn until_reads(&mut self, name: &str, key: &str, value: &Value) {
        let end = Instant::now() + WAIT;
        loop {
            let now = self.get(name, key).await;
            if now == *value {
                return;
            }
            assert!(
                Instant::now() < end,
                "{name}: {key} still reads {now}, not {value}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Edit `config.toml` of Alpha the way an agent with a text editor does.
    fn edit_file(&mut self, changes: &[(&'static str, Value)]) {
        let path = self.lab.node("Alpha").dir.path().join("config.toml");
        let mut table: toml::Table =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for (key, value) in changes {
            let (section, name) = key.split_once('.').unwrap();
            table[section][name] = serde_json::from_value(value.clone()).unwrap();
        }
        std::fs::write(path, toml::to_string_pretty(&table).unwrap()).unwrap();
    }
}

// ---- scenario 1: status ------------------------------------------------------------------

#[given("Mouser is running")]
async fn running(w: &mut AgentWorld) {
    let beta = Setup::default();
    beta.platform
        .set_permission(Kind::Accessibility, State::Missing);
    w.desk(beta).await;
}

#[when("the agent asks the command-line tool for status")]
async fn asks_for_status(w: &mut AgentWorld) {
    // The tool sends `status` over the control socket and prints the result.
    let status = w
        .lab
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
    "it reports the desk, its computers, their connection state, missing permissions, and recent problems, in both human-readable and machine-readable form"
)]
async fn reports_everything(w: &mut AgentWorld) {
    // Machine-readable: the JSON result is what the schema describes.
    let value = w.seen[0].clone();
    let errors: Vec<_> = status_schema()
        .iter_errors(&value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let status: Status = serde_json::from_value(value).unwrap();
    assert_eq!(status.desk.as_ref().unwrap().name, "Home office");
    assert_eq!(status.computers.len(), 2);
    let alpha = status.computers.iter().find(|c| c.name == "Alpha").unwrap();
    assert_eq!(alpha.connection, ConnectionState::SelfComputer);
    let beta = status.computers.iter().find(|c| c.name == "Beta").unwrap();
    assert_eq!(beta.connection, ConnectionState::Connected);
    assert_eq!(
        beta.permissions[&PermissionKind::Accessibility],
        PermissionState::Missing
    );
    let problem = status
        .problems
        .iter()
        .find(|p| p.computer == beta.id && p.kind == ProblemKind::MissingPermission)
        .expect("the recent problem");
    // Human-readable: everything the tool prints in words is in the result
    // as words: names, states and a plain-language message per problem.
    assert!(problem.message.contains("Accessibility"), "{problem:?}");
    assert!(status.problems.iter().all(|p| !p.message.is_empty()));
    assert!(status.computers.iter().all(|c| !c.name.is_empty()));
}

// ---- scenario 2: settings via file or command line ---------------------------------------

#[given("any setting available in the GUI")]
async fn any_setting(w: &mut AgentWorld) {
    w.desk(Setup::default()).await;
    w.changes = settings(false);
}

#[when("the agent changes it in the configuration file or through the command-line tool")]
async fn changes_it(w: &mut AgentWorld) {
    // Through the tool: `config.set` for every setting.
    for (key, value) in w.changes.clone() {
        let _: Value = w
            .lab
            .node("Alpha")
            .call(Method::ConfigSet, json!({"key": key, "value": value}))
            .await;
    }
    // In the file: every setting to a different value.
    w.seen = vec![json!(w.lab.node("Alpha").events.all(Topic::Status).len())];
    let second = settings(true);
    w.edit_file(&second);
    w.changes.extend(second);
}

#[then("Mouser behaves as if it had been changed in the GUI, and the GUI shows the new value")]
async fn behaves_like_the_gui(w: &mut AgentWorld) {
    // The file's values replaced the ones set through the tool.
    let changes = w.changes.clone();
    let in_the_file = &changes[changes.len() / 2..];
    for (key, value) in in_the_file {
        w.until_reads("Alpha", key, value).await;
    }
    // The GUI shows the new name (it renders `status`).
    let alpha = w.lab.node("Alpha").status().await;
    assert_eq!(alpha.computer.name, "Renamed by the agent");
    // The settings the desk shares reached the desk-mate.
    for (key, value) in in_the_file
        .iter()
        .filter(|(k, _)| mouser_proto::config::DESK_SHARED_KEYS.contains(k))
    {
        w.until_reads("Beta", key, value).await;
    }
    // The GUI was told by `status` notifications on the way.
    let before = w.seen[0].as_u64().unwrap() as usize;
    let told = w.lab.node("Alpha").events.all(Topic::Status).len();
    assert!(told > before, "{told} status notifications after {before}");
    // Nothing was restarted: the desk-mate is still connected.
    let alpha = w.lab.node("Alpha").status().await;
    assert!(
        alpha
            .computers
            .iter()
            .any(|c| c.connection == ConnectionState::Connected)
    );
}

// ---- scenario 3: JSON log lines ----------------------------------------------------------

#[given("diagnostic logging is on")]
async fn logging_is_on(w: &mut AgentWorld) {
    w.lab.add("Alpha").await;
    let on: LoggingSetResult = w
        .lab
        .node("Alpha")
        .call(Method::LoggingSet, json!({"enabled": true}))
        .await;
    w.log_path = Some(on.path);
    // Things happen: a desk is made.
    let _: mouser_proto::control::Desk = w
        .lab
        .node("Alpha")
        .call(Method::DeskCreate, json!({"name": "Home office"}))
        .await;
}

#[when("the agent reads the log file")]
async fn reads_the_log(w: &mut AgentWorld) {
    let text = std::fs::read_to_string(w.log_path.as_ref().unwrap()).expect("the log file");
    w.seen = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON record per line"))
        .collect();
}

#[then("each line is one self-contained JSON record with a timestamp, the computer, and the event")]
async fn each_line_is_a_record(w: &mut AgentWorld) {
    assert!(!w.seen.is_empty(), "the log has lines");
    let schema = schema();
    for record in &w.seen {
        assert!(schema.is_valid(record), "{record}");
        assert!(record["ts"].as_str().is_some_and(|ts| !ts.is_empty()));
        assert_eq!(record["computer"]["name"], "Alpha");
        assert!(record["computer"]["id"].as_str().is_some());
        assert!(record["event"].as_str().is_some_and(|e| !e.is_empty()));
    }
    let events: Vec<_> = w.seen.iter().map(|r| r["event"].clone()).collect();
    assert!(events.contains(&json!("logging.enabled")), "{events:?}");
}

// ---- edge case: file edited while running ------------------------------------------------

#[given("an agent edits the configuration file while Mouser is running")]
async fn edits_while_running(w: &mut AgentWorld) {
    w.desk(Setup::default()).await;
    let id = w.lab.node("Alpha").status().await.computer.id;
    w.seen = vec![json!(id)];
    w.changes = vec![
        ("general.computer_name", json!("Renamed by the agent")),
        ("mouse.buttons_4_5", json!("back_forward")),
    ];
}

#[when("the edit is saved")]
async fn edit_is_saved(w: &mut AgentWorld) {
    let changes = w.changes.clone();
    w.edit_file(&changes);
}

#[then("the change takes effect without restarting Mouser")]
async fn takes_effect(w: &mut AgentWorld) {
    for (key, value) in w.changes.clone() {
        w.until_reads("Alpha", key, &value).await;
    }
    // It is the same running instance, still on the desk and connected.
    let alpha = w.lab.node("Alpha").status().await;
    assert_eq!(json!(alpha.computer.id), w.seen[0]);
    assert_eq!(alpha.computer.name, "Renamed by the agent");
    assert!(
        alpha
            .computers
            .iter()
            .any(|c| c.connection == ConnectionState::Connected)
    );
}
