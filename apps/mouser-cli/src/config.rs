//! `config get|set|path`: through the running Mouser, or straight on
//! `config.toml` when it is not running (Story 9 sc. 2).

use clap::Subcommand;
use mouser_core::clipboard::targeted::Chord;
use mouser_core::config::{self, Config};
use mouser_core::crossing::panic::PanicSpec;
use mouser_proto::control::Method;
use serde_json::{Value, json};

use crate::{Failure, Mouser, emit};

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print one setting, a section, or everything.
    Get {
        /// `section.name`, `section`, or nothing for all settings.
        key: Option<String>,
    },
    /// Change one setting; an invalid value prints `key: reason` and exits 3.
    Set { key: String, value: String },
    /// Print the path of config.toml.
    Path,
}

fn invalid(key: &str, reason: impl std::fmt::Display) -> Failure {
    Failure::Invalid(format!("{key}: {reason}"))
}

pub async fn run(live: Option<Mouser>, json: bool, action: ConfigCommand) -> Result<(), Failure> {
    let dir = config::default_dir();
    match action {
        ConfigCommand::Path => {
            let path = dir.join(config::FILE);
            let path = path.to_string_lossy().into_owned();
            emit(json, &json!({ "path": path }), || path.clone());
        }
        ConfigCommand::Get { key } => {
            let value = get(live, &dir, key.as_deref()).await?;
            emit(json, &value, || render(&value));
        }
        ConfigCommand::Set { key, value } => {
            let current = get(None, &dir, Some(&key)).await?;
            let typed = typed(&key, &current, &value)?;
            match live {
                Some(mut mouser) => {
                    mouser
                        .call(Method::ConfigSet, json!({ "key": key, "value": typed }))
                        .await?;
                }
                None => set_file(&dir, &key, typed)?,
            }
            emit(json, &json!({}), || format!("{key} = {value}"));
        }
    }
    Ok(())
}

/// The setting(s) from the running Mouser, or from the file.
async fn get(
    live: Option<Mouser>,
    dir: &std::path::Path,
    key: Option<&str>,
) -> Result<Value, Failure> {
    match live {
        Some(mut mouser) => mouser.call(Method::ConfigGet, json!({ "key": key })).await,
        None => lookup(&tree(&load(dir)), key).cloned(),
    }
}

/// The file's settings (a missing file, in a missing directory, is the default).
fn load(dir: &std::path::Path) -> Config {
    std::fs::create_dir_all(dir).expect("config directory can be created");
    config::load_or_create(dir)
}

fn tree(config: &Config) -> Value {
    serde_json::to_value(config).expect("config serializes")
}

fn lookup<'a>(tree: &'a Value, key: Option<&str>) -> Result<&'a Value, Failure> {
    match key {
        None => Ok(tree),
        Some(key) => key
            .split('.')
            .try_fold(tree, |node, part| node.get(part))
            .ok_or_else(|| invalid(key, "unknown key")),
    }
}

/// Strings print bare; everything else as JSON.
fn render(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(_) => serde_json::to_string_pretty(value).expect("value serializes"),
        other => other.to_string(),
    }
}

/// `text` as the JSON type the setting `current` has: a true/false, a number,
/// a comma-separated list, or a string.
fn typed(key: &str, current: &Value, text: &str) -> Result<Value, Failure> {
    match current {
        Value::Bool(_) => match text {
            "true" => Ok(json!(true)),
            "false" => Ok(json!(false)),
            _ => Err(invalid(key, "expected true or false")),
        },
        Value::Number(_) => text
            .parse::<u64>()
            .map(Value::from)
            .map_err(|_| invalid(key, "expected a number")),
        Value::Array(_) => Ok(Value::Array(
            text.split(',')
                .filter(|item| !item.is_empty())
                .map(|item| json!(item))
                .collect(),
        )),
        _ => Ok(json!(text)),
    }
}

/// The same checks the running Mouser makes, then write the file.
fn set_file(dir: &std::path::Path, key: &str, value: Value) -> Result<(), Failure> {
    let mut tree = tree(&load(dir));
    if key == "schema" {
        return Err(invalid(key, "read-only"));
    }
    if lookup(&tree, Some(key))?.is_object() {
        return Err(invalid(key, "is a section; set its keys"));
    }
    let (section, name) = key.split_once('.').expect("a leaf key has a section");
    tree[section][name] = value;
    let next: Config =
        serde_json::from_value(tree).expect("a typed value keeps the config's shape");
    next.validate().map_err(|e| invalid(&e.key, e.reason))?;
    PanicSpec::parse(&next.hotkeys.panic).map_err(|e| invalid("hotkeys.panic", e))?;
    Chord::parse(&next.hotkeys.targeted_paste).map_err(|e| invalid("hotkeys.targeted_paste", e))?;
    config::save(dir, &next);
    Ok(())
}
