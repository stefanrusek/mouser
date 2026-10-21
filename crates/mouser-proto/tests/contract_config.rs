use mouser_proto::config::{Config, DESK_SHARED_KEYS};

const CONTRACT: &str = include_str!("../../../specs/001-mouser-v1/contracts/config.md");

fn contract_toml() -> &'static str {
    let start = CONTRACT.find("```toml\n").unwrap() + "```toml\n".len();
    let end = start + CONTRACT[start..].find("```").unwrap();
    &CONTRACT[start..end]
}

#[test]
fn contract_toml_parses_and_equals_default() {
    let parsed: Config = toml::from_str(contract_toml()).unwrap();
    assert_eq!(parsed, Config::default());
}

#[test]
fn default_values_match_contract() {
    let c = Config::default();
    assert_eq!(c.schema, 1);
    assert!(c.general.start_at_login && c.general.notifications);
    assert_eq!(c.general.computer_name, "");
    assert_eq!(c.hotkeys.enable_disable, "Ctrl+Alt+Shift+M");
    assert_eq!(c.hotkeys.quickjump, "Ctrl+Alt+Shift+J");
    assert_eq!(c.hotkeys.targeted_paste, "Ctrl+Alt+Shift+V");
    assert_eq!(c.hotkeys.panic, "Escape*2");
    assert_eq!(c.mouse.buttons_4_5, "passthrough");
    assert!(c.crossing.block_fullscreen && !c.crossing.equalize_dimensions);
    assert!(!c.diagnostics.logging);
    assert_eq!(c.diagnostics.max_log_mb, 50);
    assert!(c.updates.automatic);
    assert_eq!(
        c.updates.release_url,
        "https://api.github.com/repos/<owner>/mouser/releases/latest"
    );
    assert_eq!(c.advanced.port, 47801);
    assert!(c.advanced.allowlist.is_empty());
    assert_eq!(c.validate(), Ok(()));
}

#[test]
fn config_roundtrips_through_toml() {
    let mut c = Config::default();
    c.advanced.allowlist = vec!["10.0.0.0/8".into(), "fd00::/8".into()];
    let text = toml::to_string(&c).unwrap();
    assert_eq!(toml::from_str::<Config>(&text).unwrap(), c);
}

#[test]
fn validate_buttons_4_5() {
    let mut c = Config::default();
    c.mouse.buttons_4_5 = "back_forward".into();
    assert_eq!(c.validate(), Ok(()));
    c.mouse.buttons_4_5 = "swap".into();
    let e = c.validate().unwrap_err();
    assert_eq!(e.key, "mouse.buttons_4_5");
    assert_eq!(
        e.to_string(),
        "mouse.buttons_4_5: must be \"passthrough\" or \"back_forward\""
    );
}

#[test]
fn validate_port_range() {
    let mut c = Config::default();
    for ok in [1, 65535] {
        c.advanced.port = ok;
        assert_eq!(c.validate(), Ok(()));
    }
    for bad in [0, 65536] {
        c.advanced.port = bad;
        assert_eq!(
            c.validate().unwrap_err().to_string(),
            "advanced.port: must be between 1 and 65535"
        );
    }
}

#[test]
fn validate_allowlist_entries_are_cidr() {
    let mut c = Config::default();
    c.advanced.allowlist = vec!["192.168.1.0/24".into(), "fe80::/10".into()];
    assert_eq!(c.validate(), Ok(()));
    for bad in ["192.168.1.1", "nonsense", "10.0.0.0/33"] {
        c.advanced.allowlist = vec!["10.0.0.0/8".into(), bad.into()];
        assert_eq!(
            c.validate().unwrap_err().to_string(),
            format!("advanced.allowlist: {bad:?} is not a CIDR network")
        );
    }
}

#[test]
fn desk_shared_keys_are_the_crossing_and_allowlist_keys() {
    assert_eq!(
        DESK_SHARED_KEYS,
        [
            "crossing.block_fullscreen",
            "crossing.equalize_dimensions",
            "advanced.allowlist"
        ]
    );
}
