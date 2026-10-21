//! Contract tests for `mouser status [--json]` (Story 9 sc. 1): the JSON
//! validates against contracts/status.schema.json; the text names the desk, each
//! computer's connection, missing permissions and problems.

use crate::support::Lab;
use mouser_proto::control::Status;

fn validator() -> jsonschema::Validator {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../specs/001-mouser-v1/contracts/status.schema.json"
    ))
    .unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn status_json_validates_against_the_schema() {
    let validator = validator();
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;

    let alone = alpha.mouser(&["--json", "status"]).await.json();
    validator.validate(&alone).unwrap();
    assert!(alone["desk"].is_null());

    alpha.mouser(&["desk", "create", "Home office"]).await;
    let beta = lab.add("Beta").await;
    let join = beta.spawn(&["desk", "join", "Home office"]);
    let request = alpha.pending_request().await;
    alpha.mouser(&["approve", &request.to_string()]).await;
    join.wait_with_output().await.unwrap();
    alpha
        .wait_status("both connected", |s| s.computers.len() == 2)
        .await;

    for node in [&alpha, &beta] {
        let status = node.mouser(&["--json", "status"]).await.json();
        validator.validate(&status).unwrap();
        let parsed: Status = serde_json::from_value(status).unwrap();
        assert_eq!(parsed.computers.len(), 2);
        assert_eq!(parsed.desk.unwrap().name, "Home office");
    }
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn status_text_names_desk_connections_permissions_and_problems() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let nothing = alpha.mouser(&["status"]).await;
    assert_eq!(nothing.code, 0, "{}", nothing.stderr);
    assert!(nothing.stdout.starts_with("Alpha: active\n"));
    assert!(nothing.stdout.contains("Desk: none\n"));
    // The engine reports whatever the platform limits; the text lists exactly those.
    let reported = alpha.status().await.problems;
    let expected = if reported.is_empty() {
        "Problems: none\n".to_string()
    } else {
        let lines: String = reported
            .iter()
            .map(|p| {
                let fix = p.fix.map_or(String::new(), |f| format!(" (fix: {f})"));
                format!("  Alpha: {}{fix}\n", p.message)
            })
            .collect();
        format!("Problems:\n{lines}")
    };
    assert!(nothing.stdout.ends_with(&expected), "{}", nothing.stdout);
    assert!(nothing.stdout.contains("Missing permissions"));

    alpha.mouser(&["desk", "create", "Home office"]).await;
    let beta = lab.add("Beta").await;
    let join = beta.spawn(&["desk", "join", "Home office"]);
    let waiting = beta
        .wait_status("pending", |s| {
            s.desk
                .as_ref()
                .is_some_and(|d| d.pending_join == Some(true))
        })
        .await;
    let text = beta.mouser(&["status"]).await.stdout;
    assert!(
        text.contains(&format!(
            "Desk: Home office [{}] (waiting for approval)\n",
            waiting.desk.unwrap().id
        )),
        "{text}"
    );

    let request = alpha.pending_request().await;
    alpha.mouser(&["approve", &request.to_string()]).await;
    join.wait_with_output().await.unwrap();
    alpha
        .wait_status("Beta connected", |s| s.computers.len() == 2)
        .await;
    let desk_id = alpha.status().await.desk.unwrap().id;

    let text = alpha.mouser(&["status"]).await.stdout;
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "Alpha: active");
    assert_eq!(lines[1], format!("Desk: Home office [{desk_id}]"));
    assert_eq!(lines[2], "Computers:");
    assert_eq!(lines[3], "  Alpha: this computer");
    assert!(lines[4].starts_with("  Beta: connected, "), "{text}");
    assert!(
        lines.iter().any(|l| l.starts_with("Missing permissions")),
        "{text}"
    );
    assert!(lines.iter().any(|l| l.starts_with("Problems")), "{text}");

    alpha.mouser(&["pause"]).await;
    let paused = alpha.mouser(&["status"]).await.stdout;
    assert!(paused.starts_with("Alpha: paused\n"));
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn status_exits_2_when_mouser_is_not_running() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for args in [&["status"][..], &["--json", "status"]] {
        let out = nobody.mouser(args).await;
        assert_eq!(out.code, 2);
        assert_eq!(out.stdout, "");
        assert_eq!(out.stderr.trim(), "Mouser is not running");
    }
    lab.shutdown().await;
}
