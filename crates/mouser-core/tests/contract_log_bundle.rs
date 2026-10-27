//! Contract test (producer side) for contracts/log-bundle.md: a desk of three
//! computers, one of them at 0.2.8 and one with logging off, collects a
//! bundle whose `bundle.json` validates against `log-bundle.schema.json`,
//! whose entries that are not included all carry a reason, and which
//! contains none of the files that are never present (FR-007).

mod steps;
mod support;

use mouser_platform::Platform as _;
use mouser_proto::control::{LogsCollectResult, Method};
use serde_json::{Value, json};
use support::desk::{Lab, Setup};
use support::logs::{self, Zip};

fn setup(version: &str, logging: bool) -> Setup {
    let mut setup = Setup {
        mouser_version: version.into(),
        ..Setup::default()
    };
    setup.config.diagnostics.logging = logging;
    setup
}

#[tokio::test]
async fn the_bundle_of_a_desk_with_an_old_computer_and_a_quiet_one_follows_the_contract() {
    let mut lab = Lab::new();
    lab.form_desk(
        "Home office",
        "Alpha",
        &["Beta", "Gamma"],
        vec![
            ("Alpha", setup("0.3.0", true)),
            ("Beta", setup("0.3.0", false)),
            ("Gamma", setup("0.2.8", true)),
        ],
    )
    .await;
    // Files that must never travel sit in the folder the logs come from.
    for name in ["Alpha", "Beta", "Gamma"] {
        let logs = lab.node(name).dir.path().join("logs");
        std::fs::create_dir_all(logs.join("secrets")).unwrap();
        std::fs::write(logs.join("state.json"), b"{\"never\": true}").unwrap();
        std::fs::write(logs.join("secrets").join("device.key"), b"DEVICE-KEY-BYTES").unwrap();
        std::fs::write(logs.join("desk.key"), b"DESK-KEY-BYTES").unwrap();
    }
    let keys = {
        let alpha = lab.node("Alpha");
        let secrets = alpha.platform.secrets();
        let desk: Value = alpha.call_raw(Method::Status, json!({})).await.unwrap();
        let desk_id = desk["desk"]["id"].as_str().unwrap().to_string();
        vec![
            secrets
                .get(mouser_core::identity::DEVICE_KEY)
                .unwrap()
                .unwrap(),
            secrets
                .get(&format!("desk_key/{desk_id}"))
                .unwrap()
                .unwrap(),
        ]
    };

    let out = tempfile::tempdir().unwrap();
    let result: LogsCollectResult = lab
        .node("Alpha")
        .call(
            Method::LogsCollect,
            json!({"dir": out.path().to_str().unwrap()}),
        )
        .await;
    let mut zip = Zip::open(std::path::Path::new(&result.path));
    let manifest = zip.manifest();
    if let Err(e) = logs::bundle_validator().validate(&manifest) {
        panic!("bundle.json: {e}\n{manifest}");
    }
    let computers = manifest["computers"].as_array().unwrap();
    assert_eq!(computers.len(), 3);
    for entry in computers {
        if entry["included"] == false {
            assert!(entry["reason"].is_string(), "{entry}");
        }
    }
    let reason = |name: &str| {
        computers
            .iter()
            .find(|c| c["name"] == name)
            .map(|c| c["reason"].clone())
    };
    assert_eq!(reason("Alpha"), Some(Value::Null));
    assert_eq!(reason("Beta"), Some(json!("logging_off")));
    assert_eq!(reason("Gamma"), Some(json!("older_version")));

    // None of the never-present files, by name or by content.
    let names = zip.names();
    for name in &names {
        for banned in ["state.json", "secrets", "desk.key", "device.key"] {
            assert!(!name.contains(banned), "{name}");
        }
    }
    for name in &names {
        let bytes = zip.bytes(name);
        let text = String::from_utf8_lossy(&bytes);
        for decoy in ["DEVICE-KEY-BYTES", "DESK-KEY-BYTES", "never"] {
            assert!(!text.contains(decoy), "{name} holds {decoy}");
        }
        for key in &keys {
            assert!(
                !bytes.windows(key.len()).any(|w| w == key.as_slice()),
                "{name} holds a key"
            );
            let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
            assert!(!text.contains(&hex), "{name} holds a key in hex");
        }
    }
    lab.shutdown().await;
}
