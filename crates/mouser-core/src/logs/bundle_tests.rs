//! The bundle file (contracts/log-bundle.md): its name, where it goes, what
//! it holds and what it never holds.

use std::io::{Cursor, Read};
use std::path::Path;

use chrono::{TimeZone, Utc};
use mouser_proto::control::{BundleReason, ConnectionState};
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::Os;
use mouser_proto::wire::ComputerMode;
use serde_json::{Value, json};

use super::bundle::*;
use crate::paths::Env;

const ME: ComputerId = ComputerId([0xaa; 16]);
const BETA: ComputerId = ComputerId([0xbb; 16]);
const GAMMA: ComputerId = ComputerId([0xcc; 16]);
const DESK: DeskId = DeskId([0xdd; 16]);

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 27, 15, 30, 45).unwrap()
}

#[test]
fn the_file_is_named_after_the_desk_and_the_time_in_utc() {
    assert_eq!(
        file_name(Some("Home_office-2"), at()),
        "mouser-logs-Home_office-2-20261027T153045Z.zip"
    );
}

#[test]
fn every_character_outside_the_safe_set_of_the_desk_name_becomes_a_dash() {
    assert_eq!(
        file_name(Some("Büro / #1: \"x\""), at()),
        "mouser-logs-B-ro----1---x--20261027T153045Z.zip"
    );
}

#[test]
fn without_a_desk_the_name_says_so() {
    assert_eq!(
        file_name(None, at()),
        "mouser-logs-no-desk-20261027T153045Z.zip"
    );
}

fn env() -> Env {
    Env {
        home: Some("/home/ann".into()),
        appdata: None,
        xdg_config_home: None,
    }
}

#[test]
fn the_bundle_goes_to_the_folder_asked_for() {
    for mode in [ComputerMode::Installed, ComputerMode::Headless] {
        assert_eq!(
            location(
                Some(Path::new("/tmp/out")),
                mode,
                Path::new("/var/mouser"),
                &env()
            ),
            Path::new("/tmp/out")
        );
    }
}

#[test]
fn otherwise_it_goes_to_the_downloads_folder_of_the_user() {
    for mode in [
        ComputerMode::Installed,
        ComputerMode::Portable,
        ComputerMode::PortableElevated,
        ComputerMode::Service,
    ] {
        assert_eq!(
            location(None, mode, Path::new("/var/mouser"), &env()),
            Path::new("/home/ann/Downloads")
        );
    }
}

#[test]
fn a_headless_machine_engine_keeps_bundles_in_its_machine_folder() {
    let expected = if cfg!(windows) {
        Path::new("/home/ann/Downloads")
    } else {
        Path::new("/var/mouser/bundles")
    };
    assert_eq!(
        location(
            None,
            ComputerMode::Headless,
            Path::new("/var/mouser"),
            &env()
        ),
        expected
    );
}

#[test]
fn a_name_already_taken_gets_a_number_before_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let name = "mouser-logs-Home-20261027T153045Z.zip";
    assert_eq!(unique_path(dir.path(), name), dir.path().join(name));
    std::fs::write(dir.path().join(name), b"x").unwrap();
    let second = dir.path().join("mouser-logs-Home-20261027T153045Z (2).zip");
    assert_eq!(unique_path(dir.path(), name), second);
    std::fs::write(&second, b"x").unwrap();
    assert_eq!(
        unique_path(dir.path(), name),
        dir.path().join("mouser-logs-Home-20261027T153045Z (3).zip")
    );
}

fn computer(id: ComputerId, name: &str) -> Computer {
    Computer {
        id,
        name: name.into(),
        os: Os::Macos,
        os_version: Some("15.1".into()),
        version: "0.3.0".into(),
        mode: Some(ComputerMode::Installed),
        connection: ConnectionState::Connected,
        included: true,
        reason: None,
        config_toml: Some(format!("# config of {name}\n")),
        files: vec![
            ("mouser.log".into(), format!("log of {name}\n").into_bytes()),
            ("mouser.log.1".into(), b"older\n".to_vec()),
        ],
    }
}

fn mine() -> Computer {
    Computer {
        connection: ConnectionState::SelfComputer,
        ..computer(ME, "Alpha Mac")
    }
}

fn entries(zip: &[u8]) -> Vec<(String, zip::CompressionMethod)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
    (0..archive.len())
        .map(|i| {
            let file = archive.by_index(i).unwrap();
            (file.name().unwrap().to_string(), file.compression())
        })
        .collect()
}

fn text(zip: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
    let mut out = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    out
}

#[test]
fn the_zip_holds_the_manifest_then_each_computers_folder_requester_first_and_only_deflates() {
    let zip = build(
        at(),
        ME,
        Some((DESK, "Home office")),
        &[mine(), computer(BETA, "Beta")],
    );
    let found = entries(&zip);
    let names: Vec<_> = found.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "bundle.json",
            "Alpha-Mac-aaaaaaaa/computer.json",
            "Alpha-Mac-aaaaaaaa/config.toml",
            "Alpha-Mac-aaaaaaaa/mouser.log",
            "Alpha-Mac-aaaaaaaa/mouser.log.1",
            "Beta-bbbbbbbb/computer.json",
            "Beta-bbbbbbbb/config.toml",
            "Beta-bbbbbbbb/mouser.log",
            "Beta-bbbbbbbb/mouser.log.1",
        ]
    );
    assert!(
        found
            .iter()
            .all(|(_, method)| *method == zip::CompressionMethod::Deflated)
    );
    assert_eq!(text(&zip, "Beta-bbbbbbbb/mouser.log"), "log of Beta\n");
    assert_eq!(
        text(&zip, "Beta-bbbbbbbb/config.toml"),
        "# config of Beta\n"
    );
    let card: Value = serde_json::from_str(&text(&zip, "Beta-bbbbbbbb/computer.json")).unwrap();
    assert_eq!(
        card,
        json!({
            "id": BETA.to_string(), "name": "Beta", "os": "macos", "os_version": "15.1",
            "version": "0.3.0", "mode": "installed", "connection": "connected"
        })
    );
}

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/log-bundle.schema.json"
    ))
    .unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

#[test]
fn the_manifest_lists_every_computer_with_what_the_requester_knows_and_why_some_are_missing() {
    let offline = Computer {
        os_version: None,
        mode: None,
        connection: ConnectionState::Offline,
        included: false,
        reason: Some(BundleReason::Offline),
        config_toml: None,
        files: Vec::new(),
        ..computer(GAMMA, "Gamma")
    };
    let quiet = Computer {
        included: false,
        reason: Some(BundleReason::LoggingOff),
        files: Vec::new(),
        ..computer(BETA, "Beta")
    };
    let zip = build(
        at(),
        ME,
        Some((DESK, "Home office")),
        &[mine(), quiet, offline],
    );
    let manifest: Value = serde_json::from_str(&text(&zip, "bundle.json")).unwrap();
    assert!(validator().is_valid(&manifest), "{manifest}");
    assert_eq!(manifest["created_at"], "2026-10-27T15:30:45Z");
    assert_eq!(manifest["requested_by"], ME.to_string());
    assert_eq!(
        manifest["desk"],
        json!({"id": DESK.to_string(), "name": "Home office"})
    );
    let computers = manifest["computers"].as_array().unwrap();
    assert_eq!(computers.len(), 3);
    assert_eq!(computers[0]["id"], ME.to_string());
    assert_eq!(computers[0]["connection"], "self");
    assert_eq!(computers[0]["included"], true);
    assert_eq!(computers[0]["folder"], "Alpha-Mac-aaaaaaaa");
    assert_eq!(computers[0]["settings_file"], "config.toml");
    assert_eq!(computers[0]["files"], json!(["mouser.log", "mouser.log.1"]));
    assert_eq!(computers[1]["reason"], "logging_off");
    assert_eq!(computers[1]["settings_file"], "config.toml");
    assert_eq!(computers[1]["files"], json!([]));
    assert_eq!(computers[2]["reason"], "offline");
    assert!(computers[2].get("settings_file").is_none());
    assert!(computers[2].get("os_version").is_none());
    assert!(computers[2].get("mode").is_none());
    // Only what is known is in the folder.
    let names: Vec<_> = entries(&zip).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Gamma-cccccccc/computer.json".to_string()));
    assert!(!names.contains(&"Gamma-cccccccc/config.toml".to_string()));
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("Beta-bbbbbbbb/mouser.log"))
    );
}

#[test]
fn a_computer_on_no_desk_has_a_null_desk_in_the_manifest() {
    let zip = build(at(), ME, None, &[mine()]);
    let manifest: Value = serde_json::from_str(&text(&zip, "bundle.json")).unwrap();
    assert!(validator().is_valid(&manifest));
    assert_eq!(manifest["desk"], Value::Null);
}

#[test]
fn state_secrets_and_keys_are_never_in_the_zip_even_when_a_peer_names_them() {
    let mut beta = computer(BETA, "Beta");
    for name in [
        "state.json",
        "secrets/device.key",
        "secrets/desk_key_dd",
        "desk.key",
        "device.key",
        "../escape.log",
        "mouser.log.2",
    ] {
        beta.files.push((name.into(), b"SECRET".to_vec()));
    }
    beta.files
        .push(("service.log".into(), b"service\n".to_vec()));
    let zip = build(at(), ME, Some((DESK, "Home")), &[mine(), beta]);
    let names: Vec<_> = entries(&zip).into_iter().map(|(n, _)| n).collect();
    for banned in [
        "state.json",
        "secrets",
        "desk.key",
        "device.key",
        "escape",
        ".log.2",
    ] {
        assert!(
            !names.iter().any(|n| n.contains(banned)),
            "{banned}: {names:?}"
        );
    }
    assert!(names.contains(&"Beta-bbbbbbbb/service.log".to_string()));
    let manifest: Value = serde_json::from_str(&text(&zip, "bundle.json")).unwrap();
    assert_eq!(
        manifest["computers"][1]["files"],
        json!(["mouser.log", "mouser.log.1", "service.log"])
    );
    assert!(validator().is_valid(&manifest));
}

#[test]
fn the_summary_is_what_logs_collect_answers() {
    let quiet = Computer {
        included: false,
        reason: Some(BundleReason::OlderVersion),
        ..computer(BETA, "Beta")
    };
    let summary = summary(&[mine(), quiet]);
    assert_eq!(
        serde_json::to_value(&summary).unwrap(),
        json!([
            {"id": ME.to_string(), "name": "Alpha Mac", "included": true},
            {"id": BETA.to_string(), "name": "Beta", "included": false, "reason": "older_version"}
        ])
    );
}
