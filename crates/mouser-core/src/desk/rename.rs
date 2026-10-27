//! `desk.rename`: the desk-wide friendly name of a computer (Story 4
//! scenario 3). The name lives in `Member.display_name` of the desk state,
//! so every computer shows it; the computer it names also keeps it in its
//! own `general.computer_name`.

use std::sync::Arc;

use crate::lock::LockExt;

use mouser_proto::control::{DeskRenameParams, ErrorData, ProblemKind, RpcError, SettingOrigin};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;

use crate::config::{self, changes};
use crate::control::invalid;
use crate::engine::{Core, Ctx, Phase};

/// The longest friendly name, in characters.
const LONGEST: usize = 64;

impl Core {
    /// What the desk calls the connected `computer`: its member name, or
    /// the name it introduced itself by when it is not a member.
    pub(crate) fn name_of(&self, computer: ComputerId) -> String {
        self.state
            .desk
            .as_ref()
            .and_then(|d| d.state.members.iter().find(|m| m.computer_id == computer))
            .map_or_else(
                || self.peers[&computer].hello.display_name.clone(),
                |m| m.display_name.clone(),
            )
    }
}

impl Ctx {
    /// `desk.rename`.
    pub(crate) fn rename(self: &Arc<Self>, params: DeskRenameParams) -> Result<(), RpcError> {
        let name = params.name.trim();
        let mut core = self.core.guard();
        let desk = core.state.desk.as_ref().ok_or_else(|| RpcError {
            code: -32001,
            message: "This computer is not on a desk.".into(),
            data: Some(ErrorData {
                kind: ProblemKind::JoinRefused,
            }),
        })?;
        if !(1..=LONGEST).contains(&name.chars().count()) {
            return Err(invalid("A name is 1 to 64 characters."));
        }
        if !desk
            .state
            .members
            .iter()
            .any(|m| m.computer_id == params.computer)
        {
            return Err(invalid("That computer is not a member of this desk."));
        }
        self.set_member_name(&mut core, params.computer, name);
        let mine = params.computer == self.me();
        if mine {
            self.store_own_name(&mut core, name, SettingOrigin::Cli, None);
        }
        drop(core);
        if mine {
            self.announce();
        }
        self.notify_status();
        Ok(())
    }

    /// `who`'s member entry takes `name`, under a new epoch that goes to
    /// every desk-mate. The computer must be on a desk.
    pub(crate) fn set_member_name(&self, core: &mut Core, who: ComputerId, name: &str) {
        let desk = core.state.desk.as_mut().expect("on a desk");
        desk.state
            .members
            .iter_mut()
            .find(|m| m.computer_id == who)
            .expect("a member")
            .display_name = name.into();
        desk.state.epoch += 1;
        desk.state.author = self.me();
        let state = desk.state.clone();
        let _ = self.persist(core);
        for peer in core.peers.values().filter(|p| p.phase == Phase::Authed) {
            peer.send(ControlMessage::DeskState(state.clone()));
        }
    }

    /// This computer's `general.computer_name` becomes `name`, the name the
    /// desk gave it.
    pub(crate) fn store_own_name(
        &self,
        core: &mut Core,
        name: &str,
        origin: SettingOrigin,
        from: Option<ComputerId>,
    ) {
        let before = core.config.clone();
        core.config.general.computer_name = name.into();
        config::save(&self.config_dir, &core.config);
        self.log.apply(
            core.config.diagnostics.logging,
            core.config.diagnostics.max_log_mb,
            &self.display_name_in(core),
        );
        self.publish_changes(changes::diff(&before, &core.config, origin, from));
    }
}
