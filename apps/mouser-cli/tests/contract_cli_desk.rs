//! Contract tests for the desk and approval commands of contracts/cli.md: the
//! real `mouser` binary against in-process engines serving their control socket.

mod support;

use std::time::Duration;

use mouser_proto::control::Method;
use serde_json::Value;
use support::{Lab, Node, Output};

async fn finished(child: tokio::process::Child) -> Output {
    let out = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output())
        .await
        .expect("mouser finishes")
        .unwrap();
    Output {
        code: out.status.code().unwrap(),
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

/// Create "Home office" on `owner`; its desk id.
async fn create_desk(owner: &Node, name: &str) -> String {
    let created = owner
        .mouser(&["--json", "desk", "create", name])
        .await
        .json();
    created["desk_id"].as_str().unwrap().to_string()
}

/// `joiner` joins by `target`; `owner` approves; the join's output.
async fn join_approved(owner: &Node, joiner: &Node, args: &[&str]) -> Output {
    let join = joiner.spawn(args);
    let request = owner.pending_request().await;
    let approved = owner.mouser(&["approve", &request.to_string()]).await;
    assert_eq!(approved.code, 0);
    finished(join).await
}

#[tokio::test(flavor = "multi_thread")]
async fn every_command_exits_2_when_mouser_is_not_running() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for args in [
        &["desks"][..],
        &["--json", "desks"],
        &["desk", "create", "Home"],
        &["desk", "join", "Home"],
        &["desk", "leave"],
        &["desk", "remove", "Beta"],
        &["requests"],
        &["approve", "1"],
        &["decline", "1"],
    ] {
        let out = nobody.mouser(args).await;
        assert_eq!(out.code, 2, "{args:?}");
        assert_eq!(out.stdout, "");
        assert_eq!(out.stderr.trim(), "Mouser is not running", "{args:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unusable_arguments_exit_3_and_help_exits_0() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for args in [
        &[][..],
        &["bogus"],
        &["approve", "first"],
        &["decline", "-1"],
        &["desk", "join"],
        &["desks", "--bogus"],
    ] {
        let out = nobody.mouser(args).await;
        assert_eq!(out.code, 3, "{args:?}");
        assert_eq!(out.stdout, "");
        assert!(
            out.stderr.starts_with("error:") || out.stderr.starts_with("Control"),
            "{args:?}: {}",
            out.stderr
        );
    }
    let help = nobody.mouser(&["--help"]).await;
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("Control a running Mouser"));
    for command in ["desks", "desk", "requests", "approve", "decline"] {
        assert!(help.stdout.contains(command), "help lists {command}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_created_desk_is_listed_on_the_network() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let beta = lab.add("Beta").await;
    let nothing = beta.mouser(&["desks"]).await;
    assert_eq!((nothing.code, nothing.stdout.as_str()), (0, "\n"));
    assert_eq!(
        beta.mouser(&["--json", "desks"]).await.json(),
        Value::Array(vec![])
    );

    let created = alpha.mouser(&["desk", "create", "Home office"]).await;
    assert_eq!(created.code, 0, "{}", created.stderr);
    assert!(created.stdout.starts_with("Created desk \"Home office\" ["));
    let status = alpha.status().await;
    let desk_id = status.desk.unwrap().id.to_string();

    let text = beta.mouser(&["desks"]).await;
    assert_eq!(text.code, 0);
    assert_eq!(text.stdout, format!("Home office [{desk_id}]: Alpha\n"));
    let listed = beta.mouser(&["--json", "desks"]).await.json();
    assert_eq!(listed[0]["desk_id"], desk_id.as_str());
    assert_eq!(listed[0]["name"], "Home office");
    assert_eq!(listed[0]["members"][0], "Alpha");
}

#[tokio::test(flavor = "multi_thread")]
async fn join_waits_for_approval_and_prints_the_decision() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let beta = lab.add("Beta").await;
    let desk_id = create_desk(&alpha, "Home office").await;

    // By name, approved from `requests`/`approve`.
    let join = beta.spawn(&["desk", "join", "Home office"]);
    let request = alpha.pending_request().await;
    // Unrelated notifications while the join waits do not end it.
    beta.wait_status("the join is pending", |s| {
        s.desk
            .as_ref()
            .is_some_and(|d| d.pending_join == Some(true))
    })
    .await;
    beta.call(Method::ActivePause).await;
    beta.call(Method::ActiveResume).await;
    let listed = alpha.mouser(&["requests"]).await;
    assert_eq!(
        listed.stdout,
        format!("{request}: Beta wants to join Home office\n")
    );
    let approved = alpha.mouser(&["approve", &request.to_string()]).await;
    assert_eq!(
        (approved.code, approved.stdout.as_str()),
        (0, &*format!("Approved request {request}\n"))
    );
    let joined = finished(join).await;
    assert_eq!(joined.code, 0, "{}", joined.stderr);
    assert_eq!(
        joined.stdout,
        format!("Joined desk \"Home office\" [{desk_id}]\n")
    );
    let on_beta = beta.status().await;
    assert_eq!(on_beta.desk.unwrap().name, "Home office");
    alpha
        .wait_status("Beta is a member", |s| s.computers.len() == 2)
        .await;
    assert_eq!(
        alpha.mouser(&["--json", "requests"]).await.json(),
        Value::Array(vec![])
    );

    // Removed by name; the removed computer is off the desk.
    let removed = alpha.mouser(&["desk", "remove", "Beta"]).await;
    assert_eq!(
        (removed.code, removed.stdout.as_str()),
        (0, "Removed Beta from the desk\n")
    );
    alpha
        .wait_status("Beta removed", |s| s.computers.len() == 1)
        .await;
    beta.wait_status("Beta off the desk", |s| s.desk.is_none())
        .await;

    // By desk id, with JSON output.
    let joined = join_approved(&alpha, &beta, &["--json", "desk", "join", &desk_id]).await;
    let desk = joined.json();
    assert_eq!(desk["id"], desk_id.as_str());
    assert_eq!(desk["name"], "Home office");

    // Removed by computer id, with JSON output.
    let beta_id = beta.id().to_string();
    let removed = alpha.mouser(&["--json", "desk", "remove", &beta_id]).await;
    assert_eq!(removed.json(), serde_json::json!({}));
    beta.wait_status("Beta off the desk", |s| s.desk.is_none())
        .await;

    // By network address, then leave.
    let address = beta.sim.route_to(&alpha.sim).to_string();
    let joined = join_approved(&alpha, &beta, &["desk", "join", &address]).await;
    assert_eq!(joined.code, 0, "{}", joined.stderr);
    assert!(joined.stdout.starts_with("Joined desk \"Home office\""));
    let left = beta.mouser(&["desk", "leave"]).await;
    assert_eq!((left.code, left.stdout.as_str()), (0, "Left the desk\n"));
    beta.wait_status("Beta off the desk", |s| s.desk.is_none())
        .await;
    let left = alpha.mouser(&["--json", "desk", "leave"]).await;
    assert_eq!(left.json(), serde_json::json!({}));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_declined_join_exits_1_with_the_reason() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let beta = lab.add("Beta").await;
    create_desk(&alpha, "Home office").await;
    let join = beta.spawn(&["desk", "join", "Home office"]);
    let request = alpha.pending_request().await;
    let declined = alpha.mouser(&["decline", &request.to_string()]).await;
    assert_eq!(
        (declined.code, declined.stdout.as_str()),
        (0, &*format!("Declined request {request}\n"))
    );
    let join = finished(join).await;
    assert_eq!(join.code, 1);
    assert_eq!(join.stdout, "");
    assert!(join.stderr.contains("not admitted"), "{}", join.stderr);
    assert!(beta.status().await.desk.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn arguments_that_name_nothing_exit_3() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let beta = lab.add("Beta").await;
    let gamma = lab.add("Gamma").await;
    create_desk(&alpha, "Home office").await;
    create_desk(&gamma, "Home office").await;

    let unknown = beta.mouser(&["desk", "join", "No such desk"]).await;
    assert_eq!(unknown.code, 3);
    assert!(unknown.stderr.contains("no desk named \"No such desk\""));

    let ambiguous = beta.mouser(&["desk", "join", "Home office"]).await;
    assert_eq!(ambiguous.code, 3);
    assert!(
        ambiguous
            .stderr
            .contains("several desks are named \"Home office\"")
    );
    assert!(ambiguous.stderr.contains("Alpha") && ambiguous.stderr.contains("Gamma"));

    let nobody = alpha.mouser(&["desk", "remove", "Nobody"]).await;
    assert_eq!(nobody.code, 3);
    assert!(nobody.stderr.contains("no computer named \"Nobody\""));

    let missing = alpha.mouser(&["approve", "999"]).await;
    assert_eq!(missing.code, 3);
    assert!(missing.stderr.contains("no such join request"));
    assert_eq!(alpha.mouser(&["decline", "999"]).await.code, 3);

    let already = alpha.mouser(&["desk", "join", "10.0.0.1:1"]).await;
    assert_eq!(already.code, 3);
    assert!(already.stderr.contains("leave the current desk first"));
}
