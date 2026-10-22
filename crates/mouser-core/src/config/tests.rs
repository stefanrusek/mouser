use super::*;

#[test]
fn missing_file_yields_the_default_and_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let config = load_or_create(dir.path());
    assert_eq!(config, Config::default());
    let written = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
    assert_eq!(
        toml::from_str::<Config>(&written).unwrap(),
        Config::default()
    );
}

#[test]
fn existing_file_is_parsed() {
    let dir = tempfile::tempdir().unwrap();
    let mut custom = Config::default();
    custom.advanced.port = 51000;
    custom.advanced.allowlist = vec!["192.168.0.0/16".into()];
    custom.general.computer_name = "Studio".into();
    std::fs::write(dir.path().join(FILE), toml::to_string(&custom).unwrap()).unwrap();
    assert_eq!(load_or_create(dir.path()), custom);
}

#[test]
fn an_invalid_file_starts_with_the_defaults_and_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = Config::default();
    bad.advanced.port = 0;
    let text = toml::to_string(&bad).unwrap();
    std::fs::write(dir.path().join(FILE), &text).unwrap();
    let loaded = load(dir.path());
    assert_eq!(loaded.config, Config::default());
    let rejection = loaded.rejection.unwrap();
    assert_eq!(
        rejection.reason,
        "advanced.port: must be between 1 and 65535"
    );
    assert_eq!(rejection.text, text);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
        text
    );
    assert_eq!(load_or_create(dir.path()), Config::default());
}

#[test]
fn a_file_that_is_not_toml_or_has_the_wrong_kind_of_value_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    for text in ["this is [not toml", "[advanced]\nport = \"x\"\n"] {
        std::fs::write(dir.path().join(FILE), text).unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.config, Config::default(), "{text}");
        assert!(!loaded.rejection.unwrap().reason.is_empty(), "{text}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            text
        );
    }
}

#[test]
fn a_hotkey_the_engine_cannot_parse_rejects_the_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(FILE),
        "[hotkeys]\nquickjump = \"Hyper+J\"\n",
    )
    .unwrap();
    let loaded = load(dir.path());
    assert_eq!(loaded.config, Config::default());
    assert!(
        loaded
            .rejection
            .unwrap()
            .reason
            .starts_with("hotkeys.quickjump: ")
    );
}

#[test]
fn missing_keys_take_their_defaults() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(FILE),
        "[advanced]\nport = 51000\n\n[general]\ncomputer_name = \"Studio\"\n",
    )
    .unwrap();
    let loaded = load(dir.path());
    assert!(loaded.rejection.is_none());
    let mut expected = Config::default();
    expected.advanced.port = 51000;
    expected.general.computer_name = "Studio".into();
    assert_eq!(loaded.config, expected);
    std::fs::write(dir.path().join(FILE), "").unwrap();
    assert_eq!(load_or_create(dir.path()), Config::default());
}

#[test]
fn validate_checks_every_hotkey_the_engine_parses() {
    assert!(validate(&Config::default()).is_ok());
    for (key, value) in [
        ("hotkeys.panic", "Escape"),
        ("hotkeys.targeted_paste", "Nope"),
        ("hotkeys.quickjump", "Hyper+J"),
    ] {
        let err = with_value(&Config::default(), key, serde_json::json!(value))
            .err()
            .unwrap();
        assert_eq!(err.key, key);
    }
}

#[test]
fn with_value_changes_one_leaf_or_says_why_not() {
    let next = with_value(
        &Config::default(),
        "advanced.port",
        serde_json::json!(50000),
    )
    .unwrap()
    .0;
    assert_eq!(next.advanced.port, 50000);
    for (key, value, shown) in [
        ("nope.key", serde_json::json!(1), "nope.key: unknown key"),
        ("schema", serde_json::json!(2), "schema: read-only"),
        (
            "crossing",
            serde_json::json!(1),
            "crossing: is a section; set its keys",
        ),
        (
            "advanced.port",
            serde_json::json!(0),
            "advanced.port: must be between 1 and 65535",
        ),
    ] {
        let err = with_value(&Config::default(), key, value).err().unwrap();
        assert_eq!(err.to_string(), shown);
    }
    let err = with_value(&Config::default(), "advanced.port", serde_json::json!("x"))
        .err()
        .unwrap();
    assert!(err.to_string().starts_with("advanced.port: "));
}

#[test]
fn default_dir_is_the_per_os_mouser_directory() {
    let dir = default_dir();
    let name = dir.file_name().unwrap().to_string_lossy().to_lowercase();
    assert_eq!(name, "mouser");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_dir_is_under_application_support() {
    let dir = dir_from(&|k| (k == "HOME").then(|| "/Users/me".to_string()));
    assert_eq!(
        dir,
        std::path::PathBuf::from("/Users/me/Library/Application Support/Mouser")
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_dir_is_under_appdata() {
    let dir = dir_from(&|k| (k == "APPDATA").then(|| r"C:\Users\me\AppData\Roaming".to_string()));
    assert_eq!(
        dir,
        std::path::PathBuf::from(r"C:\Users\me\AppData\Roaming").join("Mouser")
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn linux_dir_prefers_xdg_config_home() {
    let xdg = dir_from(&|k| match k {
        "XDG_CONFIG_HOME" => Some("/xdg".to_string()),
        _ => None,
    });
    assert_eq!(xdg, std::path::PathBuf::from("/xdg/mouser"));
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn linux_falls_back_to_home_dot_config() {
    let home = dir_from(&|k| (k == "HOME").then(|| "/home/me".to_string()));
    assert_eq!(home, std::path::PathBuf::from("/home/me/.config/mouser"));
}

#[test]
#[should_panic(expected = "cannot read")]
fn unreadable_file_is_fatal() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(FILE)).unwrap();
    load_or_create(dir.path());
}

#[test]
fn the_placeholder_release_url_written_by_0_1_is_replaced_with_the_default() {
    // 0.1.x wrote a default with a literal `<owner>` that can never resolve.
    let dir = tempfile::tempdir().unwrap();
    let mut old = Config::default();
    old.updates.release_url = "https://api.github.com/repos/<owner>/mouser/releases/latest".into();
    old.advanced.port = 51000;
    save(dir.path(), &old);
    let loaded = load_or_create(dir.path());
    assert_eq!(
        loaded.updates.release_url,
        Config::default().updates.release_url
    );
    assert_eq!(loaded.advanced.port, 51000, "nothing else changes");
    let written = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
    assert_eq!(toml::from_str::<Config>(&written).unwrap(), loaded);
}

#[test]
fn a_release_url_the_user_chose_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut custom = Config::default();
    custom.updates.release_url = "https://example.test/latest.json".into();
    save(dir.path(), &custom);
    assert_eq!(load_or_create(dir.path()), custom);
}
