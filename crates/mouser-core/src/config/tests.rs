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
#[should_panic(expected = "config.toml: advanced.port: must be between 1 and 65535")]
fn invalid_existing_file_is_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = Config::default();
    bad.advanced.port = 0;
    std::fs::write(dir.path().join(FILE), toml::to_string(&bad).unwrap()).unwrap();
    load_or_create(dir.path());
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
    let dir = dir_from(|k| (k == "HOME").then(|| "/Users/me".to_string()));
    assert_eq!(
        dir,
        std::path::PathBuf::from("/Users/me/Library/Application Support/Mouser")
    );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_dir_is_under_appdata() {
    let dir = dir_from(|k| (k == "APPDATA").then(|| r"C:\Users\me\AppData\Roaming".to_string()));
    assert_eq!(
        dir,
        std::path::PathBuf::from(r"C:\Users\me\AppData\Roaming").join("Mouser")
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn linux_dir_prefers_xdg_config_home() {
    let xdg = dir_from(|k| match k {
        "XDG_CONFIG_HOME" => Some("/xdg".to_string()),
        _ => None,
    });
    assert_eq!(xdg, std::path::PathBuf::from("/xdg/mouser"));
    let home = dir_from(|k| (k == "HOME").then(|| "/home/me".to_string()));
    assert_eq!(home, std::path::PathBuf::from("/home/me/.config/mouser"));
}

#[test]
#[should_panic(expected = "cannot read")]
fn unreadable_file_is_fatal() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(FILE)).unwrap();
    load_or_create(dir.path());
}
