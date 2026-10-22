use std::path::Path;

use mouser_platform::{Capability, Limitation, Os, Platform, PlatformInfo, SessionType};
use mouser_proto::control::{Desk, Method, ProblemKind};
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use crate::paths::{Env, Paths};
use crate::portable::WINDOWS_LIMITATION;
use crate::testing::*;

fn info(os: Os) -> PlatformInfo {
    PlatformInfo {
        os,
        version: "virtual".into(),
        session: SessionType::Native,
        limitations: vec![Limitation {
            capability: Capability::Inject,
            text: "inject text".into(),
        }],
    }
}

/// A portable layout on a drive; returns the drive and its paths.
fn portable_drive() -> (tempfile::TempDir, Paths) {
    let drive = tempfile::tempdir().unwrap();
    std::fs::create_dir(drive.path().join("MouserData")).unwrap();
    let env = Env {
        home: None,
        appdata: None,
        xdg_config_home: None,
    };
    let paths = Paths::resolve(&drive.path().join("mouser"), Os::Linux, &env);
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(free_port());
    crate::config::save(&paths.dir, &config);
    (drive, paths)
}

fn portable(paths: &Paths) -> impl FnOnce(&mut crate::EngineParams) + '_ {
    move |params| {
        let configured = std::mem::replace(params, placeholder());
        *params = configured.with_paths(paths);
    }
}

fn placeholder() -> crate::EngineParams {
    crate::EngineParams::new(
        std::sync::Arc::new(VirtualPlatform::new()),
        Path::new("unused").into(),
        Path::new("unused").into(),
        String::new(),
        String::new(),
    )
}

#[test]
fn paths_become_the_engines_directory_and_secrets() {
    let (_drive, paths) = portable_drive();
    let params = placeholder().with_paths(&paths);
    assert_eq!(params.config_dir, paths.dir);
    assert!(params.portable);
    assert!(params.secrets.is_some());
    let installed = Paths {
        dir: "x".into(),
        portable: false,
    };
    let params = placeholder().with_paths(&installed);
    assert!(!params.portable);
    assert!(params.secrets.is_none());
}

#[tokio::test]
async fn portable_never_enables_start_at_login_and_keeps_its_state_on_the_drive() {
    let lab = Lab::new();
    let (drive, paths) = portable_drive();
    let platform = VirtualPlatform::new();
    let node = lab
        .boot_with(
            "Alpha",
            platform.clone(),
            tempfile::tempdir().unwrap(),
            portable(&paths),
        )
        .await;
    // start_at_login is on by default; creating the desk would enable it.
    let _: Desk = call(&node, Method::DeskCreate, json!({"name": "Home office"})).await;
    // Changing the setting is no different.
    let _: serde_json::Value = call(
        &node,
        Method::ConfigSet,
        json!({"key": "general.start_at_login", "value": true}),
    )
    .await;
    assert!(!platform.is_autostart_enabled());
    // Everything is on the drive, nothing in the computer's own directory
    // or secret store.
    let data = drive.path().join("MouserData");
    for file in ["config.toml", "state.json"] {
        assert!(data.join(file).is_file(), "{file}");
    }
    assert!(data.join("secrets").is_dir());
    assert_eq!(
        platform.secrets().get(crate::identity::DEVICE_KEY).unwrap(),
        None
    );
    let own: Vec<_> = std::fs::read_dir(node.dir.path()).unwrap().collect();
    assert!(
        own.iter().all(|e| {
            let name = e.as_ref().unwrap().file_name();
            name == "mouser.sock"
        }),
        "{own:?}"
    );
}

#[tokio::test]
async fn an_installed_mouser_still_registers_for_start_at_login() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let _: Desk = call(&node, Method::DeskCreate, json!({"name": "Home office"})).await;
    assert!(node.platform.is_autostart_enabled());
}

async fn limitation_texts(os: Os, portable_mode: bool) -> (Vec<String>, Vec<String>) {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_info(info(os));
    let (_drive, paths) = portable_drive();
    let node = if portable_mode {
        lab.boot_with(
            "Alpha",
            platform,
            tempfile::tempdir().unwrap(),
            portable(&paths),
        )
        .await
    } else {
        lab.start_with("Alpha", platform, |_| {}).await
    };
    let s = status(&node).await;
    let problems = s
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::PlatformLimitation)
        .map(|p| p.message.clone())
        .collect();
    (problems, s.computers[0].limitations.clone())
}

#[tokio::test]
async fn portable_windows_states_what_it_cannot_control() {
    let (mut problems, mut listed) = limitation_texts(Os::Windows, true).await;
    let mut want = vec!["inject text".to_string(), WINDOWS_LIMITATION.to_string()];
    want.sort();
    problems.sort();
    listed.sort();
    assert_eq!(problems, want);
    assert_eq!(listed, want);
}

#[tokio::test]
async fn only_portable_windows_adds_a_limitation() {
    let only = vec!["inject text".to_string()];
    assert_eq!(
        limitation_texts(Os::Windows, false).await,
        (only.clone(), only.clone())
    );
    assert_eq!(
        limitation_texts(Os::Linux, true).await,
        (only.clone(), only)
    );
}
