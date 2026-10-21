//! Configuration loading and saving at the per-OS path (contracts/config.md).

use std::path::{Path, PathBuf};

pub use mouser_proto::config::Config;

/// File name inside the config directory.
pub const FILE: &str = "config.toml";

/// Read `dir/config.toml`; a missing file yields (and writes) the default.
pub fn load_or_create(dir: &Path) -> Config {
    let path = dir.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let config: Config = toml::from_str(&text).expect("config.toml is valid TOML");
            config
                .validate()
                .unwrap_or_else(|e| panic!("config.toml: {e}"));
            config
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = Config::default();
            save(dir, &config);
            config
        }
        Err(e) => panic!("cannot read {}: {e}", path.display()),
    }
}

pub fn save(dir: &Path, config: &Config) {
    let text = toml::to_string_pretty(config).expect("config serializes");
    std::fs::write(dir.join(FILE), text).expect("config.toml writable");
}

/// The per-OS configuration directory of this user.
pub fn default_dir() -> PathBuf {
    dir_from(|key| std::env::var(key).ok())
}

#[cfg(target_os = "macos")]
fn dir_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    let home = env("HOME").expect("HOME is set");
    PathBuf::from(home).join("Library/Application Support/Mouser")
}

#[cfg(target_os = "windows")]
fn dir_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    PathBuf::from(env("APPDATA").expect("APPDATA is set")).join("Mouser")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn dir_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    match env("XDG_CONFIG_HOME") {
        Some(xdg) => PathBuf::from(xdg).join("mouser"),
        None => PathBuf::from(env("HOME").expect("HOME is set")).join(".config/mouser"),
    }
}

#[cfg(test)]
mod tests;
