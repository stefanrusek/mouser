//! `status`: the desk, each computer's connection, missing permissions and problems.

use mouser_proto::control::{ConnectionState, EngineState, Method, Status};
use mouser_proto::model::PermissionState;
use serde::Serialize;
use serde_json::json;

use crate::{Failure, Mouser};

/// The serialized (snake_case) name of an enum value.
fn name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("value serializes")
        .as_str()
        .expect("serializes as a string")
        .to_string()
}

pub async fn run(mouser: &mut Mouser) -> Result<(), Failure> {
    let value = mouser.call(Method::Status, json!({})).await?;
    let status: Status = crate::decode(value.clone())?;
    mouser.print(&value, || text(&status));
    Ok(())
}

fn text(status: &Status) -> String {
    let mut lines = vec![format!(
        "{}: {}",
        status.computer.name,
        match status.state {
            EngineState::Active => "active",
            EngineState::Paused => "paused",
        }
    )];
    lines.push(match &status.desk {
        None => "Desk: none".into(),
        Some(desk) if desk.pending_join == Some(true) => {
            format!("Desk: {} [{}] (waiting for approval)", desk.name, desk.id)
        }
        Some(desk) => format!("Desk: {} [{}]", desk.name, desk.id),
    });
    lines.push(format!(
        "Connection: {}",
        name(&status.tray_summary.connection)
    ));
    if !status.computers.is_empty() {
        lines.push("Computers:".into());
    }
    for computer in &status.computers {
        let version = format!("version {}", computer.version);
        let mode = computer
            .mode
            .map_or(String::new(), |m| format!(", {}", name(&m)));
        lines.push(match computer.connection {
            ConnectionState::SelfComputer => {
                format!("  {}: this computer, {version}{mode}", computer.name)
            }
            connection => format!(
                "  {}: {}, {}, {version}{mode}, {} display(s)",
                computer.name,
                name(&connection),
                name(&computer.os),
                computer.displays
            ),
        });
    }
    let missing: Vec<String> = status
        .computers
        .iter()
        .flat_map(|c| {
            c.permissions
                .iter()
                .filter(|(_, state)| **state == PermissionState::Missing)
                .map(|(kind, _)| format!("  {}: {}", c.name, name(kind)))
        })
        .collect();
    section(&mut lines, "Missing permissions", missing);
    let problems: Vec<String> = status
        .problems
        .iter()
        .map(|p| {
            let computer = status
                .computers
                .iter()
                .find(|c| c.id == p.computer)
                .map_or_else(|| p.computer.to_string(), |c| c.name.clone());
            let fix = p.fix.map_or(String::new(), |f| format!(" (fix: {f})"));
            format!("  {computer}: {}{fix}", p.message)
        })
        .collect();
    section(&mut lines, "Problems", problems);
    lines.join("\n")
}

/// `Title: none`, or `Title:` followed by its lines.
fn section(lines: &mut Vec<String>, title: &str, items: Vec<String>) {
    if items.is_empty() {
        lines.push(format!("{title}: none"));
    } else {
        lines.push(format!("{title}:"));
        lines.extend(items);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mouser_proto::control::{
        ComputerRef, FixAction, Problem, ProblemKind, StatusComputer, StatusDesk, TrayConnection,
    };
    use mouser_proto::ids::{ComputerId, DeskId};
    use mouser_proto::model::{Os, PermissionKind};
    use mouser_proto::wire::ComputerMode;

    use super::*;

    fn computer(name: &str, connection: ConnectionState, displays: u32) -> StatusComputer {
        StatusComputer {
            id: ComputerId([name.as_bytes()[0]; 16]),
            name: name.into(),
            os: Os::Macos,
            os_version: None,
            version: "0.1.0".into(),
            mode: None,
            connection,
            rtt_ms: None,
            last_seen: None,
            permissions: BTreeMap::new(),
            limitations: vec![],
            displays,
        }
    }

    fn problem(computer: ComputerId, message: &str, fix: Option<FixAction>) -> Problem {
        Problem {
            problem_id: "p".into(),
            kind: ProblemKind::MissingPermission,
            computer,
            message: message.into(),
            app: None,
            fix,
            raised_at: "2026-01-01T00:00:00Z".into(),
            cleared_at: None,
        }
    }

    fn status(computers: Vec<StatusComputer>, problems: Vec<Problem>) -> Status {
        Status {
            // v2 placeholder: T192
            tray_summary: mouser_proto::control::TraySummary {
                state: EngineState::Paused,
                connection: mouser_proto::control::TrayConnection::Alone,
                computers: 1,
                displays: 1,
            },
            version: "0.1.0".into(),
            computer: ComputerRef {
                id: ComputerId([0; 16]),
                name: "Alpha".into(),
            },
            state: EngineState::Paused,
            desk: None,
            active_computer: ComputerId([0; 16]),
            computers,
            problems,
            logging: None,
        }
    }

    #[test]
    fn a_computer_alone_without_a_desk() {
        assert_eq!(
            text(&status(vec![], vec![])),
            "Alpha: paused\nDesk: none\nConnection: alone\nMissing permissions: none\nProblems: none"
        );
    }

    #[test]
    fn a_pending_join_is_waiting_for_approval() {
        let mut waiting = status(vec![], vec![]);
        waiting.desk = Some(StatusDesk {
            id: DeskId([7; 16]),
            name: "Home".into(),
            pending_join: Some(true),
        });
        assert!(
            text(&waiting)
                .contains("Desk: Home [07070707070707070707070707070707] (waiting for approval)\n")
        );
    }

    #[test]
    fn lists_connections_missing_permissions_and_problems() {
        let mut beta = computer("Beta", ConnectionState::Offline, 2);
        beta.permissions
            .insert(PermissionKind::Accessibility, PermissionState::Missing);
        beta.permissions
            .insert(PermissionKind::InputMonitoring, PermissionState::Granted);
        let mut gamma = computer("Gamma", ConnectionState::Connected, 1);
        gamma.mode = Some(ComputerMode::Service);
        let stranger = ComputerId([99; 16]);
        let problems = vec![
            problem(
                beta.id,
                "Beta needs Accessibility",
                Some(FixAction::ApproveJoin),
            ),
            problem(gamma.id, "Gamma is slow", None),
            problem(stranger, "unknown", None),
        ];
        let mut full = status(
            vec![
                computer("Alpha", ConnectionState::SelfComputer, 1),
                beta,
                gamma,
            ],
            problems,
        );
        full.tray_summary.connection = TrayConnection::Disconnected;
        let desk_id = DeskId([7; 16]);
        full.desk = Some(StatusDesk {
            id: desk_id,
            name: "Home".into(),
            pending_join: None,
        });
        assert_eq!(
            text(&full),
            format!(
                "Alpha: paused\nDesk: Home [{desk_id}]\nConnection: disconnected\nComputers:\n  \
                 Alpha: this computer, version 0.1.0\n  \
                 Beta: offline, macos, version 0.1.0, 2 display(s)\n  \
                 Gamma: connected, macos, version 0.1.0, service, 1 display(s)\n\
                 Missing permissions:\n  Beta: accessibility\nProblems:\n  \
                 Beta: Beta needs Accessibility (fix: approve_join)\n  Gamma: Gamma is slow\n  \
                 {stranger}: unknown"
            )
        );
    }
}
