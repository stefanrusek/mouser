//! Watching `config.toml` (edge case of Story 9): an edit made while Mouser
//! runs applies without a restart, exactly as if the setting had been
//! changed in the GUI or with `config.set`. An edit that is not valid is
//! refused as a whole: the previous values stay in effect and the
//! diagnostic log says why.
//!
//! The file is looked at on the engine's reconcile tick. What the engine
//! saved itself parses to the config it already has, so it is not an edit.

use std::sync::Arc;

use mouser_proto::config::{Config, DESK_SHARED_KEYS};
use mouser_proto::log::LogLevel;
use serde_json::{Map, Value};

use super::FILE;
use crate::control::config_api::check;
use crate::engine::Ctx;

impl Ctx {
    /// Apply the file if its text changed since `seen`, which becomes the
    /// text looked at.
    pub(crate) fn reload_config(self: &Arc<Self>, seen: &mut String) {
        // Read with the config locked: a save by the engine itself cannot
        // slip in between the read and the comparison and be undone.
        let core = self.core.lock().unwrap();
        let Ok(text) = std::fs::read_to_string(self.config_dir.join(FILE)) else {
            return;
        };
        if text == *seen {
            return;
        }
        *seen = text.clone();
        let parsed = toml::from_str::<Config>(&text)
            .map_err(|e| e.to_string())
            .and_then(|next| {
                let hotkeys = check(&next).map_err(|e| e.to_string())?;
                Ok((next, hotkeys))
            });
        let (next, hotkeys) = match parsed {
            Ok(parsed) => parsed,
            Err(reason) => {
                let mut fields = Map::new();
                fields.insert("reason".into(), Value::String(reason));
                self.log
                    .record(LogLevel::Warn, "config.rejected", None, Some(fields));
                return;
            }
        };
        if next == core.config {
            return;
        }
        let old = serde_json::to_value(&core.config).expect("config serializes");
        let new = serde_json::to_value(&next).expect("config serializes");
        let shared = DESK_SHARED_KEYS.iter().any(|key| {
            let (section, name) = key.split_once('.').expect("a key has a section");
            old[section][name] != new[section][name]
        });
        self.apply_config(core, next, hotkeys, shared, false);
    }
}
