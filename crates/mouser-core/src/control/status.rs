//! The `status` result (contracts/status.schema.json).

use std::collections::BTreeMap;

use mouser_platform::{PermissionKind as PlatformKind, PermissionState as PlatformState};
use mouser_proto::control::{
    ComputerRef, ConnectionState, EngineState, LoggingStatus, Status, StatusComputer, StatusDesk,
};
use mouser_proto::model::{PermissionKind, PermissionState};

use crate::engine::Ctx;
use crate::state::Membership;

pub(crate) fn permission_kind(kind: PlatformKind) -> PermissionKind {
    match kind {
        PlatformKind::Accessibility => PermissionKind::Accessibility,
        PlatformKind::InputMonitoring => PermissionKind::InputMonitoring,
        PlatformKind::ScreenRecording => PermissionKind::ScreenRecording,
        PlatformKind::FullDiskAccess => PermissionKind::FullDiskAccess,
        PlatformKind::InputCapturePortal => PermissionKind::InputCapturePortal,
        PlatformKind::RemoteDesktopPortal => PermissionKind::RemoteDesktopPortal,
    }
}

pub(crate) fn permission_state(state: PlatformState) -> PermissionState {
    match state {
        PlatformState::Granted => PermissionState::Granted,
        PlatformState::Missing => PermissionState::Missing,
        PlatformState::NotApplicable => PermissionState::NotApplicable,
    }
}

/// What the menu bar or system tray shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraySummary {
    pub state: EngineState,
    pub computers: u32,
    pub displays: u32,
}

/// Active or paused, and how many computers and displays are on the desk.
pub fn tray_summary(status: &Status) -> TraySummary {
    TraySummary {
        state: status.state,
        computers: status.computers.len() as u32,
        displays: status.computers.iter().map(|c| c.displays).sum(),
    }
}

/// The `status` result.
pub(crate) fn build(ctx: &Ctx) -> Status {
    ctx.sync_permission_problems();
    let info = ctx.platform.info();
    let permissions: BTreeMap<PermissionKind, PermissionState> = ctx
        .platform
        .permissions()
        .into_iter()
        .map(|(k, s)| (permission_kind(k), permission_state(s)))
        .collect();
    let my_displays = ctx.platform.displays().expect("displays readable").len() as u32;
    let core = ctx.core.lock().unwrap();
    let now = std::time::Instant::now();
    let name = ctx.display_name_in(&core);
    let mut computers = vec![StatusComputer {
        id: ctx.me(),
        name: name.clone(),
        os: crate::platform_os(info.os),
        version: ctx.mouser_version.clone(),
        connection: ConnectionState::SelfComputer,
        rtt_ms: None,
        last_seen: None,
        permissions,
        limitations: info.limitations.into_iter().map(|l| l.text).collect(),
        displays: my_displays,
    }];
    if let Some(desk) = &core.state.desk {
        for member in desk
            .state
            .members
            .iter()
            .filter(|m| m.computer_id != ctx.me())
        {
            let peer = core.peers.get(&member.computer_id);
            let connection = core.connection_of(member.computer_id, now);
            computers.push(StatusComputer {
                id: member.computer_id,
                name: member.display_name.clone(),
                os: member.os,
                version: member.mouser_version.clone(),
                connection,
                rtt_ms: peer
                    .filter(|_| connection == ConnectionState::Connected)
                    .map(|p| p.conn.rtt().as_secs_f64() * 1000.0),
                last_seen: peer.map(|p| p.last_seen.clone()).or_else(|| {
                    core.absent
                        .get(&member.computer_id)
                        .map(|a| a.last_seen.clone())
                }),
                permissions: peer
                    .and_then(|p| p.status.as_ref())
                    .map(|s| s.permissions.clone())
                    .unwrap_or_default(),
                limitations: peer
                    .and_then(|p| p.status.as_ref())
                    .map(|s| s.limitations.clone())
                    .unwrap_or_default(),
                displays: peer.map_or(0, |p| p.displays.len() as u32),
            });
        }
    }
    let desk = match (&core.state.membership, &core.state.desk) {
        (Membership::Unjoined, _) => None,
        (Membership::Pending(id), _) => Some(StatusDesk {
            id: *id,
            name: core
                .pending
                .as_ref()
                .and_then(|p| p.name.clone())
                .unwrap_or_default(),
            pending_join: Some(true),
        }),
        (Membership::Member(id), desk) => Some(StatusDesk {
            id: *id,
            name: desk.as_ref().expect("a member has a desk").name.clone(),
            pending_join: None,
        }),
    };
    let state = if core.paused {
        EngineState::Paused
    } else {
        EngineState::Active
    };
    let mut problems = ctx.problems.open();
    for reported in core
        .peers
        .values()
        .filter_map(|p| p.status.as_ref())
        .flat_map(|s| &s.problems)
    {
        let known = problems
            .iter()
            .any(|p| p.problem_id == reported.problem_id && p.computer == reported.computer);
        if !known {
            problems.push(reported.clone());
        }
    }
    drop(core);
    Status {
        version: ctx.mouser_version.clone(),
        computer: ComputerRef { id: ctx.me(), name },
        state,
        desk,
        active_computer: ctx.crossing.lock().unwrap().active(),
        computers,
        problems,
        logging: Some(LoggingStatus {
            enabled: ctx.log.enabled(),
            path: ctx.log.path().to_string_lossy().into_owned(),
        }),
    }
}
