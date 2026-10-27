//! `config.get` / `config.set` (Story 9 scenario 2, FR-005, contracts/config.md).
//!
//! A key is `section.name`; a whole section (or no key at all) can be read.
//! A write is validated with `config::with_value` (the one validator, shared
//! with the command line) before anything changes; then it is saved,
//! applied to the running engine, and, for the desk-shared keys, replicated
//! to the desk with a new epoch.

use crate::lock::LockExt;
use std::sync::{Arc, MutexGuard};

use mouser_proto::config::Config;
use mouser_proto::control::{RpcError, SettingOrigin};
use mouser_proto::wire::ControlMessage;
use serde_json::Value;

use crate::config::{self, changes};
use crate::control::invalid;
use crate::crossing::buttons::Buttons45;
use crate::crossing::panic::PanicDetector;
use crate::engine::{Core, Ctx, Phase};
use crate::state::Membership;

impl Ctx {
    /// `config.get`: the value at `key`, or the whole config.
    pub(crate) fn get_config(&self, key: Option<&str>) -> Result<Value, RpcError> {
        let tree = serde_json::to_value(&self.core.guard().config).expect("config serializes");
        match key {
            None => Ok(tree),
            Some(key) => config::lookup(&tree, key)
                .cloned()
                .map_err(|e| invalid(e.to_string())),
        }
    }

    /// `config.set`: change one key, or refuse with `key: reason` and keep
    /// the previous value.
    pub(crate) fn set_config(
        self: &Arc<Self>,
        key: &str,
        value: Value,
        origin: SettingOrigin,
    ) -> Result<(), RpcError> {
        let core = self.core.guard();
        let (next, hotkeys) =
            config::with_value(&core.config, key, value).map_err(|e| invalid(e.to_string()))?;
        let shared = config::is_desk_state_key(key);
        self.apply_config(core, next, hotkeys, shared, true, origin);
        Ok(())
    }

    /// `next` (valid, its hotkeys parsed) becomes the config: saved if it
    /// was not read from the file, applied to the running engine, and, when
    /// `shared`, replicated to the desk with a new epoch.
    pub(crate) fn apply_config(
        self: &Arc<Self>,
        mut core: MutexGuard<'_, Core>,
        next: Config,
        (panic, targeted_paste, quickjump): config::Hotkeys,
        shared: bool,
        save: bool,
        origin: SettingOrigin,
    ) {
        let name_changed = next.general.computer_name != core.config.general.computer_name;
        let settings_changed = next.crossing.landing_highlight
            != core.config.crossing.landing_highlight
            || next.keyboard.command_as_control != core.config.keyboard.command_as_control;
        let changes = changes::diff(&core.config, &next, origin, None);
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
            let mut crossing = self.crossing.guard();
            crossing.buttons = Buttons45::from_config(&core.config.mouse.buttons_4_5);
            crossing.targeted_paste = targeted_paste;
            crossing.quickjump = quickjump;
            crossing.landing_highlight = core.config.crossing.landing_highlight;
            crossing.command_as_control = core.config.keyboard.command_as_control;
            if crossing.panic.spec() != panic {
                crossing.panic = PanicDetector::new(panic);
            }
        }
        if matches!(core.state.membership, Membership::Member(_)) {
            self.set_autostart(core.config.general.start_at_login);
        }
        if name_changed && core.state.desk.is_some() {
            // The desk names this computer as its owner now says.
            let name = self.display_name_in(&core);
            self.set_member_name(&mut core, self.me(), &name);
        }
        if shared {
            self.share_config(&mut core);
        }
        if settings_changed {
            self.share_settings(&mut core);
        }
        drop(core);
        self.publish_changes(changes);
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
        let (author, layout, allowlist) =
            (state.author, state.layout.clone(), state.allowlist.clone());
        let before = core.config.clone();
        core.config.crossing.block_fullscreen = layout.block_fullscreen;
        core.config.crossing.equalize_dimensions = layout.equalize_dimensions;
        core.config.advanced.allowlist = allowlist;
        config::save(&self.config_dir, &core.config);
        self.publish_changes(changes::diff(
            &before,
            &core.config,
            SettingOrigin::DeskMate,
            Some(author),
        ));
    }
}
