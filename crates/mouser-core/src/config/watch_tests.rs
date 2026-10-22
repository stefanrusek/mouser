//! Editing `config.toml` while Mouser runs (edge case of Story 9): the new
//! values apply without a restart, as if they had been set in the GUI; an
//! edit that is not valid is refused and the previous values stay.

use mouser_proto::config::Config;
use mouser_proto::control::{Method, Topic};
use serde_json::{Value, json};

use crate::crossing::buttons::Buttons45;
use crate::testing::*;

/// Change the file the way an agent with a text editor would.
fn edit(node: &TestNode, change: impl FnOnce(&mut toml::Table)) {
    let path = node.dir.path().join(crate::config::FILE);
    let mut table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut table);
    std::fs::write(path, toml::to_string_pretty(&table).unwrap()).unwrap();
}

fn reload(node: &TestNode, seen: &mut String) {
    node.engine.ctx.reload_config(seen);
}

fn config(node: &TestNode) -> Config {
    node.engine.ctx.core.lock().unwrap().config.clone()
}

fn epoch(node: &TestNode) -> u64 {
    node.engine
        .ctx
        .core
        .lock()
        .unwrap()
        .state
        .desk
        .as_ref()
        .unwrap()
        .state
        .epoch
}

/// The records of the diagnostic log, oldest first.
fn log(node: &TestNode) -> Vec<Value> {
    std::fs::read_to_string(node.engine.ctx.log.path())
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn rejections(node: &TestNode) -> Vec<Value> {
    log(node)
        .into_iter()
        .filter(|r| r["event"] == "config.rejected")
        .collect()
}

#[tokio::test]
async fn an_edit_applies_to_the_running_engine_as_the_gui_would() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut notes = node.engine.control().notifications();
    let mut seen = String::new();
    reload(&node, &mut seen);
    edit(&node, |t| {
        t["mouse"]["buttons_4_5"] = "back_forward".into();
        t["hotkeys"]["panic"] = "F5*3".into();
        t["hotkeys"]["targeted_paste"] = "Ctrl+Alt+P".into();
        t["general"]["computer_name"] = "Edited".into();
        t["general"]["start_at_login"] = false.into();
        t["diagnostics"]["logging"] = true.into();
    });
    let announced = node.discovery.announcements().len();
    reload(&node, &mut seen);
    assert_eq!(config(&node).mouse.buttons_4_5, "back_forward");
    {
        let crossing = node.engine.ctx.crossing.lock().unwrap();
        assert_eq!(crossing.buttons, Buttons45::BackForward);
        assert_eq!(
            crossing.panic.spec(),
            crate::crossing::panic::PanicSpec::parse("F5*3").unwrap()
        );
    }
    assert_eq!(node.engine.ctx.display_name(), "Edited");
    assert_eq!(node.discovery.announcements().len(), announced + 1);
    assert!(node.engine.ctx.log.enabled());
    // The GUI hears of it, and the command line reads the same values.
    let status = recv_topic(&mut notes, Topic::Status).await;
    assert_eq!(status["computer"]["name"], "Edited");
    let value: Value = call(
        &node,
        Method::ConfigGet,
        json!({"key": "mouse.buttons_4_5"}),
    )
    .await;
    assert_eq!(value, "back_forward");
    // The file is the user's text: it is not rewritten.
    let text = std::fs::read_to_string(node.dir.path().join(crate::config::FILE)).unwrap();
    assert_eq!(text, seen);
}

#[tokio::test]
async fn the_running_engine_notices_an_edit_by_itself() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    edit(&node, |t| t["mouse"]["buttons_4_5"] = "back_forward".into());
    eventually("the edit to be applied", || async {
        config(&node).mouse.buttons_4_5 == "back_forward"
    })
    .await;
}

#[tokio::test]
async fn a_file_that_is_not_changed_or_not_there_changes_nothing() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut seen = String::new();
    reload(&node, &mut seen);
    let before = config(&node);
    let mut notes = node.engine.control().notifications();
    reload(&node, &mut seen);
    // The same text again, and a file that was removed (an editor swapping it).
    std::fs::remove_file(node.dir.path().join(crate::config::FILE)).unwrap();
    reload(&node, &mut seen);
    assert_eq!(config(&node), before);
    assert!(notes.try_recv().is_err(), "nothing to report");
}

#[tokio::test]
async fn the_engines_own_saves_are_not_edits() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let mut seen = String::new();
    reload(a, &mut seen);
    let _: Value = call(
        a,
        Method::ConfigSet,
        json!({"key": "crossing.block_fullscreen", "value": false}),
    )
    .await;
    let after_set = epoch(a);
    reload(a, &mut seen);
    reload(a, &mut seen);
    assert_eq!(epoch(a), after_set, "the save was taken for an edit");
}

#[tokio::test]
async fn an_edit_of_a_desk_shared_key_reaches_the_desk_with_a_new_epoch() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut seen = String::new();
    reload(a, &mut seen);
    let before = epoch(a);
    edit(a, |t| {
        t["crossing"]["equalize_dimensions"] = true.into();
        t["advanced"]["allowlist"] = vec!["10.0.0.0/8"].into();
    });
    reload(a, &mut seen);
    assert_eq!(epoch(a), before + 1);
    eventually("the desk-mate to adopt the edit", || async {
        epoch(b) == before + 1
    })
    .await;
    assert!(config(b).crossing.equalize_dimensions);
    assert_eq!(config(b).advanced.allowlist, ["10.0.0.0/8"]);
    // A setting that is not shared leaves the desk alone.
    edit(a, |t| t["mouse"]["buttons_4_5"] = "back_forward".into());
    reload(a, &mut seen);
    assert_eq!(epoch(a), before + 1);
}

#[tokio::test]
async fn an_invalid_edit_is_refused_and_the_previous_values_stay() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let _: Value = call(&node, Method::LoggingSet, json!({"enabled": true})).await;
    let before = config(&node);
    let path = node.dir.path().join(crate::config::FILE);
    type Change = Box<dyn Fn(&mut toml::Table)>;
    // Not TOML, a value of the wrong kind, a value out of range, hotkeys that
    // cannot be pressed.
    let cases: Vec<(Change, &str)> = vec![
        (Box::new(|_| {}), "expected"),
        (
            Box::new(|t| t["mouse"]["buttons_4_5"] = 4.into()),
            "invalid type",
        ),
        (
            Box::new(|t| t["mouse"]["buttons_4_5"] = "sideways".into()),
            "mouse.buttons_4_5",
        ),
        (
            Box::new(|t| t["hotkeys"]["panic"] = "Escape".into()),
            "hotkeys.panic",
        ),
        (
            Box::new(|t| t["hotkeys"]["targeted_paste"] = "Nope".into()),
            "hotkeys.targeted_paste",
        ),
    ];
    for (n, (change, mentions)) in cases.into_iter().enumerate() {
        if n == 0 {
            std::fs::write(&path, "this is [not toml").unwrap();
        } else {
            crate::config::save(node.dir.path(), &before);
            edit(&node, change);
        }
        eventually("the edit to be refused", || async {
            rejections(&node).len() == n + 1
        })
        .await;
        // Said once, not at every look at the file.
        settle().await;
        let said = rejections(&node);
        assert_eq!(said.len(), n + 1, "{said:?}");
        assert_eq!(config(&node), before, "case {n}");
        let reason = said[n]["fields"]["reason"].as_str().unwrap();
        assert!(reason.contains(mentions), "{reason}");
        assert_eq!(said[n]["level"], "warn");
    }
    // Fixing the file is an edit like any other.
    let mut fixed = before.clone();
    fixed.mouse.buttons_4_5 = "back_forward".into();
    crate::config::save(node.dir.path(), &fixed);
    eventually("the fixed file to be applied", || async {
        config(&node) == fixed
    })
    .await;
}

fn problems(node: &TestNode) -> Vec<mouser_proto::control::Problem> {
    node.engine
        .ctx
        .problems
        .open()
        .into_iter()
        .filter(|p| p.problem_id == "platform_limitation:config_invalid")
        .collect()
}

#[tokio::test]
async fn a_broken_file_at_start_runs_on_the_defaults_until_it_is_fixed() {
    let lab = Lab::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(crate::config::FILE);
    let broken = "[mouse]\nbuttons_4_5 = \"sideways\"\n\n[general]\ncomputer_name = \"Fixed\"\n";
    std::fs::write(&path, broken).unwrap();
    let node = lab
        .boot_with(
            "Alpha",
            mouser_testkit::VirtualPlatform::new(),
            dir,
            |params| params.port = Some(free_port()),
        )
        .await;
    // Running on the defaults, the file as the user left it, and a problem.
    assert_eq!(config(&node), Config::default());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    let open = problems(&node);
    assert_eq!(open.len(), 1);
    assert_eq!(
        open[0].kind,
        mouser_proto::control::ProblemKind::PlatformLimitation
    );
    assert_eq!(
        open[0].message,
        format!(
            "Mouser's settings file has a mistake and is being ignored until it is fixed: \
             mouse.buttons_4_5: must be \"passthrough\" or \"back_forward\". File: {}",
            path.display()
        )
    );
    let seen: Value = call(&node, Method::Status, json!({})).await;
    assert!(
        seen["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["problem_id"] == "platform_limitation:config_invalid"),
        "{seen}"
    );
    // The reload loop does not say it again, and the file stays as it is.
    settle().await;
    assert_eq!(problems(&node).len(), 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);

    // The user fixes the mistake: it applies and the problem clears.
    std::fs::write(
        &path,
        "[mouse]\nbuttons_4_5 = \"back_forward\"\n\n[general]\ncomputer_name = \"Fixed\"\n",
    )
    .unwrap();
    eventually("the fixed file to be applied", || async {
        config(&node).mouse.buttons_4_5 == "back_forward"
    })
    .await;
    assert_eq!(config(&node).general.computer_name, "Fixed");
    assert!(problems(&node).is_empty());
}

#[tokio::test]
async fn a_still_broken_file_keeps_one_problem_with_the_newest_reason() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut seen = String::new();
    reload(&node, &mut seen);
    node.engine.ctx.log.apply(true, 50, "Alpha");
    for (text, reason) in [
        ("[advanced]\nport = 0\n", "advanced.port: must be between"),
        (
            "[hotkeys]\nquickjump = \"Hyper+J\"\n",
            "hotkeys.quickjump: ",
        ),
    ] {
        std::fs::write(node.dir.path().join(crate::config::FILE), text).unwrap();
        reload(&node, &mut seen);
        let open = problems(&node);
        assert_eq!(open.len(), 1);
        assert!(open[0].message.contains(reason), "{}", open[0].message);
    }
    let said = rejections(&node);
    assert_eq!(said.len(), 2);
    assert!(
        said[1]["fields"]["reason"]
            .as_str()
            .unwrap()
            .starts_with("hotkeys.quickjump: ")
    );
    std::fs::write(node.dir.path().join(crate::config::FILE), "").unwrap();
    reload(&node, &mut seen);
    assert!(problems(&node).is_empty());
}
