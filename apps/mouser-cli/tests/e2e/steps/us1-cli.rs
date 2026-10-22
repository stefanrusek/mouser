//! J1 steps for Story 1: install, create a desk, join it (declined, then
//! approved), two desks that stay apart, and come back after a restart
//! (quickstart.md sections 2 and 3). They drive the installed `mouser` on the
//! Mac (computer A) and the Windows VM (computer B, over SSH,
//! `$MOUSER_J1_PEER`). Journey: features/e2e_j1_two_computers.feature.

use cucumber::{given, then, when};

use crate::world::{Computer, J1World};

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The process name of the Mouser app on B, without `.exe`.
fn peer_process() -> String {
    std::env::var("MOUSER_J1_PEER_PROC").unwrap_or_else(|_| "mouser-app".into())
}

/// Launch the Mouser app on `computer` and wait until its control API answers.
fn launch(computer: Computer) {
    match computer {
        Computer::A => {
            let opened = computer.run("open", &["-a", "Mouser"]);
            assert!(
                opened.status.success(),
                "open -a Mouser: {}",
                text(&opened.stderr)
            );
        }
        Computer::B => computer.interactive(
            "app",
            &format!("Start-Process -FilePath \"{}\"", Computer::peer_app()),
        ),
    }
    computer.wait_for("Mouser to start", |c| {
        (c.mouser(&["desks"]).status.code() != Some(2)).then_some(())
    });
}

/// Quit the Mouser app on `computer` and wait until its control API is gone.
fn quit(computer: Computer) {
    let quit = match computer {
        Computer::A => computer.run("osascript", &["-e", "quit app \"Mouser\""]),
        Computer::B => computer.script(&format!(
            "Stop-Process -Name '{}' -Force -ErrorAction Stop",
            peer_process()
        )),
    };
    assert!(quit.status.success(), "quit Mouser: {}", text(&quit.stderr));
    computer.wait_for("Mouser to stop", |c| {
        (c.mouser(&["desks"]).status.code() == Some(2)).then_some(())
    });
}

/// `computer`'s `mouser status --json` once `check` accepts it.
pub(crate) fn status_where(
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

fn create(computer: Computer, name: &str) {
    let created = computer.mouser(&["desk", "create", name]);
    assert!(
        created.status.success(),
        "desk create: {}",
        text(&created.stderr)
    );
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
    create(Computer::A, &name);
}

#[when(expr = "the owner chooses Create desk {string} on computer B")]
async fn create_desk_b(_world: &mut J1World, name: String) {
    create(Computer::B, &name);
}

#[then(expr = "computer A is the only member of {string}")]
async fn a_alone(_world: &mut J1World, name: String) {
    let status = status_where(Computer::A, "computer A on its desk", |s| on_desk(s, &name));
    assert_eq!(status["computers"].as_array().unwrap().len(), 1);
}

#[then(expr = "computer B is the only member of {string}")]
async fn b_alone(_world: &mut J1World, name: String) {
    let status = status_where(Computer::B, "computer B on its desk", |s| on_desk(s, &name));
    assert_eq!(status["computers"].as_array().unwrap().len(), 1);
}

#[then("neither desk lists the other's computer as a member")]
async fn desks_apart(_world: &mut J1World) {
    let a = Computer::A.mouser_json(&["status"]);
    let b = Computer::B.mouser_json(&["status"]);
    assert_ne!(a["computer"]["id"], b["computer"]["id"]);
    assert_eq!(a["computers"][0]["id"], a["computer"]["id"]);
    assert_eq!(b["computers"][0]["id"], b["computer"]["id"]);
    assert_eq!(a["computers"].as_array().unwrap().len(), 1);
    assert_eq!(b["computers"].as_array().unwrap().len(), 1);
}

#[when("the owner removes computer B from its desk")]
async fn leave_b(_world: &mut J1World) {
    let left = Computer::B.mouser(&["desk", "leave"]);
    assert!(left.status.success(), "desk leave: {}", text(&left.stderr));
    status_where(Computer::B, "computer B to have no desk", |s| {
        s["desk"].is_null()
    });
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

fn decide(approve: bool) {
    let request = Computer::A.wait_for("a join request on computer A", |c| {
        let requests = c.mouser_json(&["requests"]);
        requests[0]["request_id"].as_u64()
    });
    let verb = if approve { "approve" } else { "decline" };
    let decided = Computer::A.mouser(&[verb, &request.to_string()]);
    assert!(
        decided.status.success(),
        "{verb}: {}",
        text(&decided.stderr)
    );
}

#[when("the owner clicks Allow on computer A")]
async fn allow(_world: &mut J1World) {
    decide(true);
}

#[when("the owner declines the request on computer A")]
async fn decline(_world: &mut J1World) {
    decide(false);
}

#[then(expr = "computer B is told it was not admitted to {string}")]
async fn not_admitted(world: &mut J1World, _name: String) {
    let join = world.join.take().expect("computer B asked to join");
    let out = join.wait_with_output().expect("join finishes");
    assert_eq!(out.status.code(), Some(1), "join: {}", text(&out.stdout));
    assert!(!text(&out.stderr).is_empty(), "the refusal is explained");
}

#[then("computer B is not a member of any desk")]
async fn b_no_desk(_world: &mut J1World) {
    status_where(Computer::B, "computer B to have no desk", |s| {
        s["desk"].is_null()
    });
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
