//! `logs.collect` (Story 2 scenario 10, contracts/control-api-delta.md).

use std::io::Read;

use mouser_proto::control::{LogsCollectResult, Method};
use serde_json::{Value, json};

use crate::testing::*;

async fn desk(versions: &[(&str, &str)]) -> (Lab, Vec<TestNode>) {
    let lab = Lab::new();
    let mut nodes = Vec::new();
    for (name, version) in versions {
        nodes.push(lab.start_version(name, version, true).await);
    }
    let nodes = desk_from(nodes).await;
    (lab, nodes)
}

fn zip_text(path: &std::path::Path, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    out
}

#[tokio::test]
async fn collect_writes_one_zip_into_the_folder_and_says_who_is_in_it_and_who_is_not() {
    let (_lab, nodes) = desk(&[("Alpha", "0.3.0"), ("Beta", "0.3.0"), ("Gamma", "0.2.8")]).await;
    let a = &nodes[0];
    let out = tempfile::tempdir().unwrap();
    let result: LogsCollectResult = call(
        a,
        Method::LogsCollect,
        json!({"dir": out.path().to_str().unwrap()}),
    )
    .await;
    let path = std::path::PathBuf::from(&result.path);
    assert_eq!(path.parent().unwrap(), out.path());
    let name = path.file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with("mouser-logs-Home-office-") && name.ends_with("Z.zip"),
        "{name}"
    );
    let written: Vec<_> = std::fs::read_dir(out.path()).unwrap().collect();
    assert_eq!(written.len(), 1, "one file");
    let listed: Vec<(String, bool, Option<String>)> = result
        .computers
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                c.included,
                c.reason.map(|r| {
                    serde_json::to_value(r)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string()
                }),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("Alpha".to_string(), true, None),
            ("Beta".to_string(), true, None),
            (
                "Gamma".to_string(),
                false,
                Some("older_version".to_string())
            ),
        ]
    );
    let manifest: Value = serde_json::from_str(&zip_text(&path, "bundle.json")).unwrap();
    assert_eq!(manifest["requested_by"], a.engine.computer_id().to_string());
    assert_eq!(manifest["desk"]["name"], "Home office");
    assert_eq!(manifest["computers"].as_array().unwrap().len(), 3);

    // The bundle is recorded in the requester's log.
    let record = logged(a, "logs.collected", |_| true).await;
    assert_eq!(record["level"], "info");
    assert_eq!(
        record["fields"],
        json!({
            "path": result.path, "included": 2,
            "missing": [{"computer": nodes[2].engine.computer_id().to_string(), "reason": "older_version"}]
        })
    );
}

#[tokio::test]
async fn a_folder_that_cannot_be_written_is_an_internal_error_with_the_reason_of_the_system() {
    let lab = Lab::new();
    let node = lab.start_version("Alpha", "0.3.0", true).await;
    // A file where the folder should go.
    let in_the_way = node.dir.path().join("in-the-way");
    std::fs::write(&in_the_way, b"x").unwrap();
    let dir = in_the_way.join("bundles");
    let error = call_err(
        &node,
        Method::LogsCollect,
        json!({"dir": dir.to_str().unwrap()}),
    )
    .await;
    assert_eq!(error.code, -32603);
    let reason = error
        .message
        .strip_prefix("The log bundle could not be written: ")
        .expect(&error.message);
    assert!(!reason.is_empty());
    assert!(log_events(&node, "logs.collected").is_empty());
}

#[cfg(not(windows))]
#[tokio::test]
async fn a_headless_machine_engine_without_a_folder_keeps_its_bundles_in_the_machine_folder() {
    let lab = Lab::new();
    let node = lab
        .start_released(
            "Alpha",
            |c| c.diagnostics.logging = true,
            |p| {
                p.mode = mouser_proto::wire::ComputerMode::Headless;
                p.mouser_version = "0.3.0".into();
            },
        )
        .await;
    let result: LogsCollectResult = call(&node, Method::LogsCollect, json!({})).await;
    let bundles = node.dir.path().join("bundles");
    assert_eq!(
        std::path::Path::new(&result.path).parent().unwrap(),
        bundles
    );
    assert!(std::path::Path::new(&result.path).is_file());
    assert_eq!(result.computers.len(), 1);
    assert!(result.computers[0].included);
    // The folder is optional: the call may carry no params at all.
    let again = node
        .engine
        .control()
        .call(Method::LogsCollect, None)
        .await
        .unwrap();
    assert!(again["path"].as_str().unwrap().contains("bundles"));
}

#[tokio::test]
async fn the_installed_service_writes_no_bundle_to_any_folder() {
    let lab = Lab::new();
    let node = lab
        .start_released(
            "Alpha",
            |c| c.diagnostics.logging = true,
            |p| {
                p.mode = mouser_proto::wire::ComputerMode::Service;
                p.mouser_version = "0.3.0".into();
            },
        )
        .await;
    let out = tempfile::tempdir().unwrap();
    for params in [json!({"dir": out.path().to_str().unwrap()}), json!({})] {
        let error = call_err(&node, Method::LogsCollect, params).await;
        assert_eq!(error.code, -32602);
        assert_eq!(
            error.message,
            "logs.collect: the installed Mouser service does not write to a chosen folder; use logs.collect_stream"
        );
    }
    // The folder is optional: the call may carry no params at all.
    let bare = node
        .engine
        .control()
        .call(Method::LogsCollect, None)
        .await
        .unwrap_err();
    assert_eq!(bare.code, -32602);
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert!(!node.dir.path().join("bundles").exists());
    assert!(log_events(&node, "logs.collected").is_empty());
}
