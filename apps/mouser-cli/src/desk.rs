//! `desks`, `desk create|join|leave|remove`, `requests`, `approve`, `decline`.

use clap::Subcommand;
use mouser_proto::control::{
    Desk, DiscoveredDesk, Method, Notification, PendingJoin, Problem, ProblemKind, Status,
    StatusDesk, Topic,
};
use mouser_proto::ids::{ComputerId, DeskId};
use serde_json::{Value, json};

use crate::{Failure, Mouser, decode};

#[derive(Subcommand)]
pub enum DeskCommand {
    /// Create a desk with this computer as its only member.
    Create { name: String },
    /// Join a desk by name, desk id or host:port, and wait for the decision.
    Join {
        /// A desk name, a desk id, or `host:port`.
        desk: String,
    },
    /// Leave the current desk.
    Leave,
    /// Remove a computer from the desk.
    Remove {
        /// A computer name or id.
        computer: String,
    },
    /// Rename a computer on the desk.
    Rename {
        /// A computer name or id.
        computer: String,
        /// The new name.
        name: String,
    },
}

pub async fn desks(mouser: &mut Mouser) -> Result<(), Failure> {
    let value = mouser.call(Method::DesksDiscovered, json!({})).await?;
    let desks: Vec<DiscoveredDesk> = decode(value.clone())?;
    mouser.print(&value, || {
        desks
            .iter()
            .map(|d| format!("{} [{}]: {}", d.name, d.desk_id, d.members.join(", ")))
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(())
}

pub async fn run(mouser: &mut Mouser, action: DeskCommand) -> Result<(), Failure> {
    match action {
        DeskCommand::Create { name } => create(mouser, name).await,
        DeskCommand::Join { desk } => join(mouser, &desk).await,
        DeskCommand::Leave => {
            let value = mouser.call(Method::DeskLeave, json!({})).await?;
            mouser.print(&value, || "Left the desk".into());
            Ok(())
        }
        DeskCommand::Remove { computer } => remove(mouser, &computer).await,
        DeskCommand::Rename { computer, name } => rename(mouser, &computer, &name).await,
    }
}

async fn create(mouser: &mut Mouser, name: String) -> Result<(), Failure> {
    let value = mouser
        .call(Method::DeskCreate, json!({ "name": name }))
        .await?;
    let desk: Desk = decode(value.clone())?;
    mouser.print(&value, || {
        format!("Created desk \"{}\" [{}]", desk.name, desk.desk_id)
    });
    Ok(())
}

/// What the owner typed for `desk join`, as `desk.join` params.
async fn join_params(mouser: &mut Mouser, target: &str) -> Result<Value, Failure> {
    if let Ok(desk_id) = target.parse::<DeskId>() {
        return Ok(json!({ "desk_id": desk_id }));
    }
    if target
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
    {
        return Ok(json!({ "address": target }));
    }
    let value = mouser.call(Method::DesksDiscovered, json!({})).await?;
    let found: Vec<DiscoveredDesk> = decode(value)?;
    let mut named = found.iter().filter(|d| d.name == target);
    match (named.next(), named.next()) {
        (None, _) => Err(Failure::Invalid(format!(
            "no desk named \"{target}\" was found on the network"
        ))),
        (Some(desk), None) => Ok(json!({ "desk_id": desk.desk_id })),
        (Some(_), Some(_)) => {
            let choices = found
                .iter()
                .filter(|d| d.name == target)
                .map(|d| format!("{} ({})", d.desk_id, d.members.join(", ")))
                .collect::<Vec<_>>()
                .join("; ");
            Err(Failure::Invalid(format!(
                "several desks are named \"{target}\"; join by desk id: {choices}"
            )))
        }
    }
}

async fn join(mouser: &mut Mouser, target: &str) -> Result<(), Failure> {
    let params = join_params(mouser, target).await?;
    mouser
        .client
        .subscribe(vec![Topic::Status, Topic::Problem])
        .await
        .map_err(Failure::Rpc)?;
    mouser.call(Method::DeskJoin, params).await?;
    let desk = loop {
        let note = mouser
            .client
            .next_notification()
            .await
            .expect("Mouser keeps the connection until the join is decided");
        if let Some(desk) = decision(note)? {
            break desk;
        }
    };
    let value = serde_json::to_value(&desk).expect("desk serializes");
    mouser.print(&value, || joined_text(&desk));
    Ok(())
}

/// The join's outcome, if `note` carries it: the desk joined (`Some`), or
/// the refusal (an error).
fn decision(note: Notification) -> Result<Option<StatusDesk>, Failure> {
    match note.method {
        Topic::Status => Ok(decode::<Status>(note.params)?
            .desk
            .filter(|desk| desk.pending_join != Some(true))),
        _ => {
            let problem: Problem = decode(note.params)?;
            let refused = problem.kind == ProblemKind::JoinRefused && problem.cleared_at.is_none();
            refused
                .then_some(problem.message)
                .map_or(Ok(None), |message| Err(Failure::Declined(message)))
        }
    }
}

fn joined_text(desk: &StatusDesk) -> String {
    format!("Joined desk \"{}\" [{}]", desk.name, desk.id)
}

/// The id of the computer the owner named, by id or by desk name.
async fn computer_id(mouser: &mut Mouser, computer: &str) -> Result<ComputerId, Failure> {
    if let Ok(id) = computer.parse::<ComputerId>() {
        return Ok(id);
    }
    let value = mouser.call(Method::Status, json!({})).await?;
    let status: Status = decode(value)?;
    status
        .computers
        .iter()
        .find(|c| c.name == computer)
        .map(|c| c.id)
        .ok_or_else(|| Failure::Invalid(format!("no computer named \"{computer}\" is on the desk")))
}

async fn remove(mouser: &mut Mouser, computer: &str) -> Result<(), Failure> {
    let id = computer_id(mouser, computer).await?;
    let value = mouser
        .call(Method::DeskRemoveMember, json!({ "computer_id": id }))
        .await?;
    mouser.print(&value, || format!("Removed {computer} from the desk"));
    Ok(())
}

async fn rename(mouser: &mut Mouser, computer: &str, name: &str) -> Result<(), Failure> {
    let id = computer_id(mouser, computer).await?;
    let value = mouser
        .call(Method::DeskRename, json!({ "computer": id, "name": name }))
        .await?;
    mouser.print(&value, || format!("Renamed {computer} to {name}"));
    Ok(())
}

pub async fn requests(mouser: &mut Mouser) -> Result<(), Failure> {
    let value = mouser.call(Method::JoinPending, json!({})).await?;
    let pending: Vec<PendingJoin> = decode(value.clone())?;
    mouser.print(&value, || {
        pending
            .iter()
            .map(|r| {
                format!(
                    "{}: {} wants to join {}",
                    r.request_id, r.computer.name, r.desk.name
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(())
}

pub async fn decide(mouser: &mut Mouser, request: u64, approve: bool) -> Result<(), Failure> {
    let value = mouser
        .call(
            Method::JoinDecide,
            json!({ "request_id": request, "approve": approve }),
        )
        .await?;
    mouser.print(&value, || {
        let verb = if approve { "Approved" } else { "Declined" };
        format!("{verb} request {request}")
    });
    Ok(())
}
