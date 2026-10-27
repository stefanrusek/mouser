//! The desk-shared settings that travel as `DeskSettings`: the landing
//! highlight and Command as Control (Story 4 scenarios 4-6). They have a
//! version of their own, `(epoch, author)`, and the higher one wins. Only
//! computers from 0.3.0 on are sent them: older ones cannot read the
//! message and behave as the defaults.

use std::sync::Arc;

use crate::lock::LockExt;

use mouser_proto::control::SettingOrigin;
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;

use crate::config::{self, changes};
use crate::engine::{Core, Ctx, Phase};
use crate::state::SettingsVersion;

impl Ctx {
    /// The settings as `DeskSettings` says them.
    fn settings_message(&self, core: &Core) -> ControlMessage {
        let version = core.state.settings;
        ControlMessage::DeskSettings {
            epoch: version.epoch,
            author: version.author.expect("changed settings have an author"),
            landing_highlight: core.config.crossing.landing_highlight,
            command_as_control: core.config.keyboard.command_as_control,
        }
    }

    /// Tell `peer` the settings, if anyone ever changed them and it can
    /// read them.
    pub(crate) fn send_settings(&self, core: &Core, peer: ComputerId) {
        let entry = &core.peers[&peer];
        if core.state.settings.epoch > 0 && entry.hello.speaks_v2() {
            entry.send(self.settings_message(core));
        }
    }

    /// The settings were changed here: they take a new version and go to
    /// every desk-mate that can read them. Off a desk they are only this
    /// computer's.
    pub(crate) fn share_settings(&self, core: &mut Core) {
        if core.state.desk.is_none() {
            return;
        }
        core.state.settings = SettingsVersion {
            epoch: core.state.settings.epoch + 1,
            author: Some(self.me()),
        };
        let _ = self.persist(core);
        let message = self.settings_message(core);
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            if peer.hello.speaks_v2() {
                peer.send(message.clone());
            }
        }
    }

    /// A desk-mate sent its settings.
    pub(crate) fn on_desk_settings(
        self: &Arc<Self>,
        peer: ComputerId,
        epoch: u64,
        author: ComputerId,
        landing_highlight: bool,
        command_as_control: bool,
    ) -> Result<(), String> {
        let mut core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("DeskSettings from a computer that is not on the desk".into());
        }
        let held = core.state.settings;
        if (epoch, Some(author)) <= (held.epoch, held.author) {
            return Ok(());
        }
        let before = core.config.clone();
        core.config.crossing.landing_highlight = landing_highlight;
        core.config.keyboard.command_as_control = command_as_control;
        core.state.settings = SettingsVersion {
            epoch,
            author: Some(author),
        };
        config::save(&self.config_dir, &core.config);
        let _ = self.persist(&core);
        {
            let mut crossing = self.crossing.guard();
            crossing.landing_highlight = landing_highlight;
            crossing.command_as_control = command_as_control;
        }
        self.publish_changes(changes::diff(
            &before,
            &core.config,
            SettingOrigin::DeskMate,
            Some(author),
        ));
        Ok(())
    }
}
