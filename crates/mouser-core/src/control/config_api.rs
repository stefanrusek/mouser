//! `config.get` / `config.set` (Story 9 scenario 2, FR-005, contracts/config.md).
//!
//! A key is `section.name`; a whole section (or no key at all) can be read.
//! A write is validated with `Config::validate` (and the panic gesture and the
//! targeted-paste hotkey, which the engine parses at start) before anything changes; then it is saved,
//! applied to the running engine, and, for the desk-shared keys, replicated
//! to the desk with a new epoch.

use std::sync::{Arc, MutexGuard};

use mouser_proto::config::{Config, ConfigError, DESK_SHARED_KEYS};
use mouser_proto::control::RpcError;
use mouser_proto::wire::ControlMessage;
use serde_json::Value;

use crate::clipboard::targeted::Chord;
use crate::config;
use crate::control::invalid;
use crate::crossing::buttons::Buttons45;
use crate::crossing::panic::{PanicDetector, PanicSpec};
use crate::engine::{Core, Ctx, Phase};
use crate::state::Membership;

fn rejected(key: &str, reason: impl std::fmt::Display) -> RpcError {
    invalid(format!("{key}: {reason}"))
}

/// The value at `key` in the config's JSON form.
fn lookup<'a>(tree: &'a Value, key: &str) -> Result<&'a Value, RpcError> {
    key.split('.')
        .try_fold(tree, |node, part| node.get(part))
        .ok_or_else(|| rejected(key, "unknown key"))
}

/// Everything a config must satisfy beyond its shape: `Config::validate`,
/// and the hotkeys the engine parses (the panic gesture and the
/// targeted-paste and QuickJump chords, which are returned parsed).
pub(crate) fn check(next: &Config) -> Result<(PanicSpec, Chord, Chord), ConfigError> {
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

impl Ctx {
    /// `config.get`: the value at `key`, or the whole config.
    pub(crate) fn get_config(&self, key: Option<&str>) -> Result<Value, RpcError> {
        let tree =
            serde_json::to_value(&self.core.lock().unwrap().config).expect("config serializes");
        match key {
            None => Ok(tree),
            Some(key) => lookup(&tree, key).cloned(),
        }
    }

    /// `config.set`: change one key, or refuse with `key: reason` and keep
    /// the previous value.
    pub(crate) fn set_config(self: &Arc<Self>, key: &str, value: Value) -> Result<(), RpcError> {
        let core = self.core.lock().unwrap();
        let mut tree = serde_json::to_value(&core.config).expect("config serializes");
        if key == "schema" {
            return Err(rejected(key, "read-only"));
        }
        if lookup(&tree, key)?.is_object() {
            return Err(rejected(key, "is a section; set its keys"));
        }
        let (section, name) = key.split_once('.').expect("a leaf key has a section");
        tree[section][name] = value;
        let next: Config =
            serde_json::from_value(tree).map_err(|e| rejected(key, e.to_string()))?;
        let hotkeys = check(&next).map_err(|e| rejected(&e.key, e.reason))?;
        let shared = DESK_SHARED_KEYS.contains(&key);
        self.apply_config(core, next, hotkeys, shared, true);
        Ok(())
    }

    /// `next` (valid, its hotkeys parsed) becomes the config: saved if it
    /// was not read from the file, applied to the running engine, and, when
    /// `shared`, replicated to the desk with a new epoch.
    pub(crate) fn apply_config(
        self: &Arc<Self>,
        mut core: MutexGuard<'_, Core>,
        next: Config,
        (panic, targeted_paste, quickjump): (PanicSpec, Chord, Chord),
        shared: bool,
        save: bool,
    ) {
        let name_changed = next.general.computer_name != core.config.general.computer_name;
        core.config = next;
        if save {
            config::save(&self.config_dir, &core.config);
        }
        self.log.apply(
            core.config.diagnostics.logging,
            core.config.diagnostics.max_log_mb,
            &self.display_name_in(&core),
        );
        {
            let mut crossing = self.crossing.lock().unwrap();
            crossing.buttons = Buttons45::from_config(&core.config.mouse.buttons_4_5);
            crossing.targeted_paste = targeted_paste;
            crossing.quickjump = quickjump;
            if crossing.panic.spec() != panic {
                crossing.panic = PanicDetector::new(panic);
            }
        }
        if matches!(core.state.membership, Membership::Member(_)) {
            self.platform
                .autostart()
                .set(core.config.general.start_at_login)
                .expect("autostart can be set");
        }
        if shared {
            self.share_config(&mut core);
        }
        drop(core);
        if name_changed {
            self.announce();
        }
        self.notify_status();
    }

    /// The desk-shared settings changed here: the desk's state takes them
    /// under a new epoch and goes to every desk-mate.
    fn share_config(&self, core: &mut Core) {
        let me = self.me();
        let Some(desk) = core.state.desk.as_mut() else {
            return;
        };
        let state = &mut desk.state;
        let old = state.layout.clone();
        state.layout.block_fullscreen = core.config.crossing.block_fullscreen;
        state.layout.equalize_dimensions = core.config.crossing.equalize_dimensions;
        state.allowlist = core.config.advanced.allowlist.clone();
        state.epoch += 1;
        state.author = me;
        let state = state.clone();
        self.layout_changed(core, &old, &state);
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::DeskState(state.clone()));
        }
    }

    /// The desk's state arrived (a join, or a desk-mate's edit): its shared
    /// settings become this computer's.
    pub(crate) fn adopt_shared_config(&self, core: &mut Core) {
        let state = &core.state.desk.as_ref().expect("on a desk").state;
        core.config.crossing.block_fullscreen = state.layout.block_fullscreen;
        core.config.crossing.equalize_dimensions = state.layout.equalize_dimensions;
        core.config.advanced.allowlist = state.allowlist.clone();
        config::save(&self.config_dir, &core.config);
    }
}
