//! Configuration loading and saving at the per-OS path (contracts/config.md).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::clipboard::targeted::Chord;
use crate::crossing::panic::PanicSpec;
pub use mouser_proto::config::{Config, ConfigError};

/// The release URL 0.1.x wrote as its default: a literal `<owner>` that
/// never resolves. A file still holding it gets the current default.
const PLACEHOLDER_RELEASE_URL: &str = "https://api.github.com/repos/<owner>/mouser/releases/latest";

/// File name inside the config directory.
pub const FILE: &str = "config.toml";

/// The hotkeys the engine parses out of a valid config: the panic gesture,
/// the targeted-paste chord and the QuickJump chord.
pub type Hotkeys = (PanicSpec, Chord, Chord);

/// A config file that was not used, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// What is wrong, in a sentence fragment (`key: reason` for a value).
    pub reason: String,
    /// The file's text, so a later look can tell an edit from the same text.
    pub text: String,
}

/// What `load` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The file's settings, or the defaults when the file was rejected.
    pub config: Config,
    pub rejection: Option<Rejection>,
}

/// Everything a config must satisfy beyond its shape: `Config::validate`,
/// and the hotkeys the engine parses (returned parsed). The one set of
/// rules for the engine at start, the live reload, `config.set` and the
/// command line's offline `config set`.
pub fn validate(next: &Config) -> Result<Hotkeys, ConfigError> {
    next.validate()?;
    let panic = PanicSpec::parse(&next.hotkeys.panic).map_err(|reason| ConfigError {
        key: "hotkeys.panic".into(),
        reason,
    })?;
    let targeted_paste =
        Chord::parse(&next.hotkeys.targeted_paste).map_err(|reason| ConfigError {
            key: "hotkeys.targeted_paste".into(),
            reason,
        })?;
    let quickjump = Chord::parse(&next.hotkeys.quickjump).map_err(|reason| ConfigError {
        key: "hotkeys.quickjump".into(),
        reason,
    })?;
    Ok((panic, targeted_paste, quickjump))
}

fn rejected(key: &str, reason: impl std::fmt::Display) -> ConfigError {
    ConfigError {
        key: key.into(),
        reason: reason.to_string(),
    }
}

/// The value at `key` in a config's JSON form.
pub fn lookup<'a>(tree: &'a Value, key: &str) -> Result<&'a Value, ConfigError> {
    key.split('.')
        .try_fold(tree, |node, part| node.get(part))
        .ok_or_else(|| rejected(key, "unknown key"))
}

/// `current` with the leaf `key` set to `value`, validated as a whole; or
/// the `key: reason` that refuses it.
pub fn with_value(
    current: &Config,
    key: &str,
    value: Value,
) -> Result<(Config, Hotkeys), ConfigError> {
    let mut tree = serde_json::to_value(current).expect("config serializes");
    if key == "schema" {
        return Err(rejected(key, "read-only"));
    }
    if lookup(&tree, key)?.is_object() {
        return Err(rejected(key, "is a section; set its keys"));
    }
    let (section, name) = key.split_once('.').expect("a leaf key has a section");
    tree[section][name] = value;
    let next: Config = serde_json::from_value(tree).map_err(|e| rejected(key, e))?;
    let hotkeys = validate(&next)?;
    Ok((next, hotkeys))
}

/// Read `dir/config.toml`; a missing file yields (and writes) the default.
/// A file that is not valid is left as it is and not used: the defaults
/// come back with the reason.
pub fn load(dir: &Path) -> Loaded {
    let path = dir.join(FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let parsed = toml::from_str::<Config>(&text)
                .map_err(|e| e.message().to_string())
                .and_then(|config| {
                    validate(&config).map_err(|e| e.to_string())?;
                    Ok(config)
                });
            match parsed {
                Ok(mut config) => {
                    if config.updates.release_url == PLACEHOLDER_RELEASE_URL {
                        config.updates.release_url = Config::default().updates.release_url;
                        save(dir, &config);
                    }
                    Loaded {
                        config,
                        rejection: None,
                    }
                }
                Err(reason) => Loaded {
                    config: Config::default(),
                    rejection: Some(Rejection { reason, text }),
                },
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let config = Config::default();
            save(dir, &config);
            Loaded {
                config,
                rejection: None,
            }
        }
        Err(e) => panic!("cannot read {}: {e}", path.display()),
    }
}

/// The settings to run with (`load` without the reason).
pub fn load_or_create(dir: &Path) -> Config {
    load(dir).config
}

pub fn save(dir: &Path, config: &Config) {
    let text = toml::to_string_pretty(config).expect("config serializes");
    std::fs::write(dir.join(FILE), text).expect("config.toml writable");
}

/// The per-OS configuration directory of this user.
pub fn default_dir() -> PathBuf {
    dir_from(&process_env)
}

fn process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[cfg(target_os = "macos")]
fn dir_from(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let home = env("HOME").expect("HOME is set");
    PathBuf::from(home).join("Library/Application Support/Mouser")
}

#[cfg(target_os = "windows")]
fn dir_from(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    PathBuf::from(env("APPDATA").expect("APPDATA is set")).join("Mouser")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn dir_from(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    match env("XDG_CONFIG_HOME") {
        Some(xdg) => PathBuf::from(xdg).join("mouser"),
        None => PathBuf::from(env("HOME").expect("HOME is set")).join(".config/mouser"),
    }
}

pub(crate) mod watch;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod watch_tests;
