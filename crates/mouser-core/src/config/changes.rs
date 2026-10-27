//! What a config apply changed (Story 2 scenario 8): one [`SettingChange`]
//! per key that differs, published on the `config` topic (the app registers
//! hotkeys again when it hears one) and written to the diagnostic log.

use mouser_proto::config::Config;
use mouser_proto::control::{Notification, SettingChange, SettingOrigin, Topic};
use mouser_proto::ids::ComputerId;

use crate::engine::Ctx;
use crate::logging::events;

/// The keys that differ between `old` and `new`, each as a change that came
/// from `origin` (from `from`, for a desk-mate), by key.
pub fn diff(
    old: &Config,
    new: &Config,
    origin: SettingOrigin,
    from: Option<ComputerId>,
) -> Vec<SettingChange> {
    let old = serde_json::to_value(old).expect("config serializes");
    let new = serde_json::to_value(new).expect("config serializes");
    let mut changes = Vec::new();
    for (section, keys) in new.as_object().expect("a config is an object") {
        // `schema` is not a section.
        let Some(keys) = keys.as_object() else {
            continue;
        };
        for (name, value) in keys {
            if old[section][name] != *value {
                changes.push(SettingChange {
                    key: format!("{section}.{name}"),
                    old: old[section][name].clone(),
                    new: value.clone(),
                    origin,
                    from,
                });
            }
        }
    }
    changes
}

impl Ctx {
    /// Tell the `config` topic and the log of `changes`.
    pub(crate) fn publish_changes(&self, changes: Vec<SettingChange>) {
        for change in changes {
            self.log.write(events::setting_changed(&change));
            // No subscribers is fine.
            let _ = self.notify.send(Notification {
                jsonrpc: "2.0".into(),
                method: Topic::Config,
                params: serde_json::to_value(&change).expect("change serializes"),
            });
        }
    }
}
