//! J1 steps for Story 1: install, create a desk, join it, approve, and come
//! back after a restart (quickstart.md sections 2 and 3). They drive the
//! installed `mouser` on two real Macs; computer B is reached over SSH
//! (`$MOUSER_J1_PEER`). The feature file is authored in Convergence.

use cucumber::{given, then, when};

use crate::world::{Computer, J1World};

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Launch the Mouser app on `computer` and wait until its control API answers.
fn launch(computer: Computer) {
    let opened = computer.run("open", &["-a", "Mouser"]);
    assert!(
        opened.status.success(),
        "open -a Mouser: {}",
        text(&opened.stderr)
    );
    computer.wait_for("Mouser to start", |c| {
        (c.mouser(&["desks"]).status.code() != Some(2)).then_some(())
    });
}

/// Quit the Mouser app on `computer` and wait until its control API is gone.
fn quit(computer: Computer) {
    let quit = computer.run("osascript", &["-e", "quit app \"Mouser\""]);
    assert!(quit.status.success(), "quit Mouser: {}", text(&quit.stderr));
    computer.wait_for("Mouser to stop", |c| {
        (c.mouser(&["desks"]).status.code() == Some(2)).then_some(())
    });
}

/// `computer`'s `mouser status --json` once `check` accepts it.
fn status_where(
    computer: Computer,
    what: &str,
    check: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    computer.wait_for(what, |c| {
        let out = c.mouser(&["--json", "status"]);
        let status: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        (out.status.success() && check(&status)).then_some(status)
    })
}

fn on_desk(status: &serde_json::Value, desk: &str) -> bool {
    status["desk"]["name"] == desk && status["desk"].get("pending_join").is_none()
}

fn sees_connected_peer(status: &serde_json::Value) -> bool {
    status["computers"]
        .as_array()
        .expect("status lists computers")
        .iter()
        .any(|c| c["connection"] == "connected")
}

#[given("Mouser is installed on both computers")]
async fn installed(_world: &mut J1World) {
    for computer in [Computer::A, Computer::B] {
        let version = computer.mouser(&["--version"]);
        assert!(
            version.status.success(),
            "mouser is not installed: {}",
            text(&version.stderr)
        );
    }
}

#[given("Mouser is launched on both computers")]
async fn launched(_world: &mut J1World) {
    launch(Computer::A);
    launch(Computer::B);
}

#[when(expr = "the owner chooses Create desk {string} on computer A")]
async fn create_desk(_world: &mut J1World, name: String) {
    let created = Computer::A.mouser(&["desk", "create", &name]);
    assert!(
        created.status.success(),
        "desk create: {}",
        text(&created.stderr)
    );
}

#[when(expr = "the owner chooses {string} on computer B")]
async fn choose_desk(world: &mut J1World, name: String) {
    Computer::B.wait_for("computer B to find the desk", |c| {
        text(&c.mouser(&["desks"]).stdout)
            .contains(&name)
            .then_some(())
    });
    world.join = Some(Computer::B.start("mouser", &["desk", "join", &name]));
}

#[when("the owner clicks Allow on computer A")]
async fn allow(_world: &mut J1World) {
    let request = Computer::A.wait_for("a join request on computer A", |c| {
        let requests = c.mouser_json(&["requests"]);
        requests[0]["request_id"].as_u64()
    });
    let approved = Computer::A.mouser(&["approve", &request.to_string()]);
    assert!(
        approved.status.success(),
        "approve: {}",
        text(&approved.stderr)
    );
}

#[then(expr = "computer B is told it joined {string}")]
async fn told_joined(world: &mut J1World, name: String) {
    let join = world.join.take().expect("computer B asked to join");
    let out = join.wait_with_output().expect("join finishes");
    assert!(out.status.success(), "join: {}", text(&out.stderr));
    assert!(text(&out.stdout).contains(&format!("Joined desk \"{name}\"")));
    world.joined = Some(out);
}

#[then(expr = "both computers list each other as connected members of {string}")]
async fn both_connected(_world: &mut J1World, name: String) {
    for computer in [Computer::A, Computer::B] {
        let status = status_where(computer, "the desk with a connected peer", |s| {
            on_desk(s, &name) && sees_connected_peer(s)
        });
        assert_eq!(status["computers"].as_array().unwrap().len(), 2);
    }
}

#[when("Mouser is restarted on computer B")]
async fn restart_b(_world: &mut J1World) {
    quit(Computer::B);
    launch(Computer::B);
}

#[then(expr = "computer B is back on {string} with no action in Mouser")]
async fn back_on_desk(_world: &mut J1World, name: String) {
    status_where(Computer::B, "computer B to rejoin its desk", |s| {
        on_desk(s, &name) && sees_connected_peer(s)
    });
}
