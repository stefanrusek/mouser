//! Contract tests for `mouser config get|set|path` (Story 9 sc. 2): through the
//! running Mouser's control API, and straight on `config.toml` when it is stopped.

use crate::support::Lab;
use mouser_core::config;
use serde_json::{Value, json};

#[tokio::test(flavor = "multi_thread")]
async fn get_reads_the_running_mousers_settings() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;

    let notifications = alpha
        .mouser(&["config", "get", "general.notifications"])
        .await;
    assert_eq!(
        (notifications.code, notifications.stdout.as_str()),
        (0, "true\n")
    );
    let hotkey = alpha.mouser(&["config", "get", "hotkeys.quickjump"]).await;
    assert_eq!(hotkey.stdout, "Ctrl+Alt+Shift+J\n");
    let port = alpha.mouser(&["config", "get", "advanced.port"]).await;
    assert_eq!(
        port.stdout.trim().parse::<u32>().unwrap(),
        alpha_port(&alpha).await
    );
    let json_value = alpha
        .mouser(&["--json", "config", "get", "hotkeys.quickjump"])
        .await
        .json();
    assert_eq!(json_value, json!("Ctrl+Alt+Shift+J"));

    let section = alpha.mouser(&["config", "get", "crossing"]).await;
    let parsed: Value = serde_json::from_str(&section.stdout).unwrap();
    assert_eq!(parsed["block_fullscreen"], json!(true));
    let all = alpha.mouser(&["--json", "config", "get"]).await.json();
    assert_eq!(all["schema"], json!(1));
    let all_text = alpha.mouser(&["config", "get"]).await;
    assert_eq!(
        serde_json::from_str::<Value>(&all_text.stdout).unwrap(),
        all
    );

    let unknown = alpha.mouser(&["config", "get", "nope.key"]).await;
    assert_eq!((unknown.code, unknown.stdout.as_str()), (3, ""));
    assert_eq!(unknown.stderr.trim(), "nope.key: unknown key");
    lab.shutdown().await;
}

async fn alpha_port(node: &crate::support::Node) -> u32 {
    node.mouser(&["--json", "config", "get", "advanced.port"])
        .await
        .json()
        .as_u64()
        .unwrap() as u32
}

#[tokio::test(flavor = "multi_thread")]
async fn set_changes_the_running_mouser_and_reports_invalid_values() {
    let lab = Lab::new();
    let alpha = lab.add("Alpha").await;

    let set = alpha
        .mouser(&["config", "set", "general.notifications", "false"])
        .await;
    assert_eq!(
        (set.code, set.stdout.as_str()),
        (0, "general.notifications = false\n")
    );
    assert_eq!(
        alpha
            .mouser(&["--json", "config", "get", "general.notifications"])
            .await
            .json(),
        json!(false)
    );
    let name = alpha
        .mouser(&["config", "set", "general.computer_name", "Desk Mac"])
        .await;
    assert_eq!(name.code, 0, "{}", name.stderr);
    assert_eq!(
        alpha
            .mouser(&["config", "get", "general.computer_name"])
            .await
            .stdout,
        "Desk Mac\n"
    );
    let list = alpha
        .mouser(&[
            "--json",
            "config",
            "set",
            "advanced.allowlist",
            "10.0.0.0/8,192.168.0.0/16",
        ])
        .await;
    assert_eq!(list.json(), json!({}));
    assert_eq!(
        alpha
            .mouser(&["--json", "config", "get", "advanced.allowlist"])
            .await
            .json(),
        json!(["10.0.0.0/8", "192.168.0.0/16"])
    );
    alpha
        .mouser(&["config", "set", "advanced.allowlist", ""])
        .await;
    assert_eq!(
        alpha
            .mouser(&["--json", "config", "get", "advanced.allowlist"])
            .await
            .json(),
        json!([])
    );
    alpha
        .mouser(&["config", "set", "advanced.port", "50000"])
        .await;
    assert_eq!(alpha_port(&alpha).await, 50000);

    for (args, reason) in [
        (
            ["mouse.buttons_4_5", "sideways"],
            "mouse.buttons_4_5: must be \"passthrough\" or \"back_forward\"",
        ),
        (
            ["advanced.port", "70000"],
            "advanced.port: must be between 1 and 65535",
        ),
        (
            ["advanced.port", "lots"],
            "advanced.port: expected a number",
        ),
        (
            ["general.notifications", "maybe"],
            "general.notifications: expected true or false",
        ),
        (["nope.key", "1"], "nope.key: unknown key"),
        (["crossing", "1"], "crossing: is a section; set its keys"),
        (["schema", "2"], "schema: read-only"),
    ] {
        let bad = alpha.mouser(&["config", "set", args[0], args[1]]).await;
        assert_eq!((bad.code, bad.stdout.as_str()), (3, ""), "{args:?}");
        assert_eq!(bad.stderr.trim(), reason, "{args:?}");
    }
    // A refused value leaves the previous one in effect.
    assert_eq!(
        alpha
            .mouser(&["config", "get", "mouse.buttons_4_5"])
            .await
            .stdout,
        "passthrough\n"
    );
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn path_names_config_toml_in_the_users_config_directory() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let expected = nobody.user_config_dir.join("config.toml");
    let path = nobody.mouser(&["config", "path"]).await;
    assert_eq!(
        (path.code, path.stdout.as_str()),
        (0, format!("{}\n", expected.display()).as_str())
    );
    let json_path = nobody.mouser(&["--json", "config", "path"]).await.json();
    assert_eq!(json_path, json!({"path": expected.to_str().unwrap()}));

    let alpha = lab.add("Alpha").await;
    assert_eq!(
        alpha.mouser(&["config", "path"]).await.stdout,
        format!("{}\n", alpha.user_config_dir.join("config.toml").display())
    );
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_mouser_stopped_get_and_set_operate_on_config_toml() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let dir = &nobody.user_config_dir;
    std::fs::create_dir_all(dir).unwrap();

    let everything = nobody.mouser(&["--json", "config", "get"]).await.json();
    assert_eq!(everything["schema"], json!(1));
    let default = nobody
        .mouser(&["config", "get", "general.notifications"])
        .await;
    assert_eq!((default.code, default.stdout.as_str()), (0, "true\n"));

    let set = nobody
        .mouser(&["config", "set", "hotkeys.targeted_paste", "Ctrl+Alt+V"])
        .await;
    assert_eq!(
        (set.code, set.stdout.as_str()),
        (0, "hotkeys.targeted_paste = Ctrl+Alt+V\n")
    );
    assert_eq!(
        config::load_or_create(dir).hotkeys.targeted_paste,
        "Ctrl+Alt+V"
    );
    let json_set = nobody
        .mouser(&["--json", "config", "set", "diagnostics.max_log_mb", "5"])
        .await;
    assert_eq!(json_set.json(), json!({}));
    assert_eq!(config::load_or_create(dir).diagnostics.max_log_mb, 5);
    assert_eq!(
        nobody
            .mouser(&["--json", "config", "get", "hotkeys.targeted_paste"])
            .await
            .json(),
        json!("Ctrl+Alt+V")
    );

    for (key, value, reason_start) in [
        (
            "mouse.buttons_4_5",
            "sideways",
            "mouse.buttons_4_5: must be",
        ),
        (
            "advanced.allowlist",
            "not-a-network",
            "advanced.allowlist: ",
        ),
        ("hotkeys.panic", "", "hotkeys.panic: "),
        ("hotkeys.targeted_paste", "", "hotkeys.targeted_paste: "),
        ("nope.key", "1", "nope.key: unknown key"),
        ("general", "1", "general: is a section; set its keys"),
        ("schema", "2", "schema: read-only"),
        ("advanced.port", "x", "advanced.port: expected a number"),
    ] {
        let bad = nobody.mouser(&["config", "set", key, value]).await;
        assert_eq!((bad.code, bad.stdout.as_str()), (3, ""), "{key}");
        assert!(
            bad.stderr.starts_with(reason_start),
            "{key}: {}",
            bad.stderr
        );
    }
    // Refused values were not written.
    let kept = config::load_or_create(dir);
    assert_eq!(kept.mouse.buttons_4_5, "passthrough");
    assert!(kept.advanced.allowlist.is_empty());
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_mouser_stopped_a_fresh_user_gets_a_config_file() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    std::fs::remove_dir_all(&nobody.user_config_dir).ok();
    let set = nobody
        .mouser(&["config", "set", "general.start_at_login", "false"])
        .await;
    assert_eq!(set.code, 0, "{}", set.stderr);
    assert!(
        !config::load_or_create(&nobody.user_config_dir)
            .general
            .start_at_login
    );
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_set_applies_the_engines_rules_to_every_hotkey_and_leaves_the_file_alone() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let dir = &nobody.user_config_dir;
    std::fs::create_dir_all(dir).unwrap();
    config::save(dir, &config::Config::default());
    let before = std::fs::read_to_string(dir.join("config.toml")).unwrap();
    for (key, value) in [
        ("hotkeys.quickjump", "Hyper+J"),
        ("hotkeys.panic", "Escape"),
        ("hotkeys.targeted_paste", "Nope"),
    ] {
        let bad = nobody.mouser(&["config", "set", key, value]).await;
        assert_eq!((bad.code, bad.stdout.as_str()), (3, ""), "{key}");
        assert!(
            bad.stderr.starts_with(&format!("{key}: ")),
            "{}",
            bad.stderr
        );
        assert!(bad.stderr.trim().len() > key.len() + 2, "{}", bad.stderr);
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            before,
            "{key} left the file unchanged"
        );
    }
    let good = nobody
        .mouser(&["config", "set", "hotkeys.quickjump", "Ctrl+Alt+K"])
        .await;
    assert_eq!(good.code, 0, "{}", good.stderr);
    lab.shutdown().await;
}

/// A copy of the `mouser` binary in a directory of its own, as a portable
/// install carries it beside `MouserData/`.
fn portable_copy(home: &std::path::Path) -> std::path::PathBuf {
    let folder = home.join("portable");
    std::fs::create_dir_all(folder.join("MouserData")).unwrap();
    let exe = folder.join(
        std::path::Path::new(env!("CARGO_BIN_EXE_mouser"))
            .file_name()
            .unwrap(),
    );
    let _no_forks = crate::support::SPAWNS.write().unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_mouser"), &exe).unwrap();
    exe
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cli_beside_a_portable_app_uses_mouser_data() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let place = tempfile::tempdir().unwrap();
    let exe = portable_copy(place.path());
    let data = exe.parent().unwrap().join("MouserData");
    let file = data.join("config.toml");

    let path = nobody.mouser_at(&exe, &["config", "path"]).await;
    assert_eq!(
        (path.code, path.stdout.as_str()),
        (0, format!("{}\n", file.display()).as_str())
    );
    let set = nobody
        .mouser_at(&exe, &["config", "set", "advanced.port", "50001"])
        .await;
    assert_eq!(set.code, 0, "{}", set.stderr);
    assert_eq!(config::load_or_create(&data).advanced.port, 50001);
    assert!(
        !nobody.user_config_dir.join("config.toml").exists(),
        "nothing was written to the installed directory"
    );
    let got = nobody
        .mouser_at(&exe, &["config", "get", "advanced.port"])
        .await;
    assert_eq!(got.stdout, "50001\n");
    let log = nobody.mouser_at(&exe, &["logs", "path"]).await;
    assert_eq!(
        log.stdout,
        format!("{}\n", data.join("logs").join("mouser.log").display())
    );
    std::fs::create_dir_all(data.join("logs")).unwrap();
    std::fs::write(data.join("logs").join("mouser.log"), "one\ntwo\n").unwrap();
    let tail = nobody.mouser_at(&exe, &["logs", "tail", "-n", "1"]).await;
    assert_eq!(tail.stdout, "two\n");
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_a_file_with_a_mistake_is_reported_and_never_overwritten() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    let dir = &nobody.user_config_dir;
    std::fs::create_dir_all(dir).unwrap();
    let file = dir.join("config.toml");
    std::fs::write(&file, "[advanced]\nport = 0\n").unwrap();
    for args in [
        vec!["config", "get"],
        vec!["config", "set", "general.start_at_login", "false"],
    ] {
        let out = nobody.mouser(&args).await;
        assert_eq!((out.code, out.stdout.as_str()), (3, ""), "{args:?}");
        assert_eq!(
            out.stderr,
            format!(
                "Mouser's settings file has a mistake: advanced.port: must be between 1 and 65535. File: {}\n",
                file.display()
            )
        );
    }
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "[advanced]\nport = 0\n"
    );
    lab.shutdown().await;
}
