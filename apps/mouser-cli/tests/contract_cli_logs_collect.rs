//! Contract tests for `mouser logs collect [--out DIR] [--json]` (Story 2
//! sc. 10, contracts/cli-delta.md, log-bundle.md): the running Mouser writes
//! the bundle; against the installed service (mode `service`, OD-7) the CLI
//! streams it with `logs.collect_stream` and writes the zip itself, as the user.

use std::path::Path;

use crate::support::Lab;
use mouser_proto::wire::ComputerMode;
use mouser_testkit::VirtualPlatform;
use serde_json::json;

/// Whether `path` holds a zip archive.
fn is_zip(path: &Path) -> bool {
    std::fs::read(path).unwrap().starts_with(b"PK")
}

#[tokio::test(flavor = "multi_thread")]
async fn collect_prints_the_path_then_a_line_per_computer_and_json_prints_the_result() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().to_str().unwrap();

    let text = alpha.mouser(&["logs", "collect", "--out", dir]).await;
    assert_eq!(text.code, 0, "{}", text.stderr);
    let lines: Vec<&str> = text.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", text.stdout);
    let path = Path::new(lines[0]);
    assert_eq!(path.parent().unwrap(), out.path());
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mouser-logs-no-desk-"),
        "{path:?}"
    );
    assert!(path.exists());
    assert!(is_zip(path));
    assert_eq!(lines[1], "Alpha: included");

    let json = alpha
        .mouser(&["--json", "logs", "collect", "--out", dir])
        .await
        .json();
    let second = Path::new(json["path"].as_str().unwrap());
    assert_ne!(second, path, "the first bundle was not overwritten");
    assert!(second.exists());
    assert_eq!(json["computers"][0]["name"], json!("Alpha"));
    assert_eq!(json["computers"][0]["included"], json!(true));
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn collect_names_each_computer_that_is_not_included_and_why() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;
    // The harness's computers report version 0.1.0, which keeps no such log.
    let beta = lab.add("Beta").await;
    alpha.mouser(&["desk", "create", "Home"]).await;
    let join = beta.spawn(&["desk", "join", "Home"]);
    let request = alpha.pending_request().await;
    alpha.mouser(&["approve", &request.to_string()]).await;
    drop(join);
    alpha
        .wait_status("beta connected", |s| {
            s.computers.iter().any(|c| {
                c.name == "Beta"
                    && c.connection == mouser_proto::control::ConnectionState::Connected
            })
        })
        .await;
    let out = tempfile::tempdir().unwrap();
    let text = alpha
        .mouser(&["logs", "collect", "--out", out.path().to_str().unwrap()])
        .await;
    assert_eq!(text.code, 0, "{}", text.stderr);
    let lines: Vec<&str> = text.stdout.lines().skip(1).collect();
    assert_eq!(
        lines,
        ["Alpha: included", "Beta: not included (older version)"]
    );
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn collect_exits_2_when_the_engine_is_not_running() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for args in [&["logs", "collect"][..], &["--json", "logs", "collect"]] {
        let out = nobody.mouser(args).await;
        assert_eq!((out.code, out.stdout.as_str()), (2, ""), "{args:?}");
        assert_eq!(out.stderr.trim(), "Mouser is not running");
    }
    lab.shutdown().await;
}

/// A computer running the installed service, in a lab of its own
/// ([`Lab::alone`]).
async fn service_node(lab: &Lab, name: &str) -> crate::support::Node {
    lab.add_tuned(
        name,
        VirtualPlatform::new(),
        |_| {},
        |params| params.mode = ComputerMode::Service,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn against_the_service_the_cli_streams_the_bundle_and_writes_it_into_out() {
    let lab = Lab::alone();
    let alpha = service_node(&lab, "Alpha").await;
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().to_str().unwrap();

    let text = alpha.mouser(&["logs", "collect", "--out", dir]).await;
    assert_eq!(text.code, 0, "{}", text.stderr);
    let lines: Vec<&str> = text.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", text.stdout);
    let path = Path::new(lines[0]);
    assert_eq!(path.parent().unwrap(), out.path());
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("mouser-logs-no-desk-")
    );
    assert!(is_zip(path));
    assert_eq!(lines[1], "Alpha: included");

    // A second bundle in the same second does not replace the first.
    let json = alpha
        .mouser(&["--json", "logs", "collect", "--out", dir])
        .await
        .json();
    let second = Path::new(json["path"].as_str().unwrap());
    assert_ne!(second, path);
    assert_eq!(second.parent().unwrap(), out.path());
    assert!(is_zip(second));
    assert_eq!(
        json["computers"],
        json!([{"id": alpha.id().to_string(), "name": "Alpha", "included": true}])
    );
    assert_eq!(
        json.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["computers", "path"]
    );
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn against_the_service_without_out_the_bundle_goes_to_the_users_downloads_folder() {
    let lab = Lab::alone();
    let alpha = service_node(&lab, "Alpha").await;
    let text = alpha.mouser(&["logs", "collect"]).await;
    assert_eq!(text.code, 0, "{}", text.stderr);
    let path = Path::new(text.stdout.lines().next().unwrap());
    assert_eq!(path.parent().unwrap(), alpha.home.join("Downloads"));
    assert!(is_zip(path));
    lab.shutdown().await;
}
