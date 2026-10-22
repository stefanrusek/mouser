//! `update [check|apply]`: look for, or install, the newest Mouser release
//! (Story 10).

use clap::Subcommand;
use mouser_proto::control::{Method, UpdateResult};
use serde_json::json;

use crate::{Failure, Mouser, decode};

#[derive(Subcommand)]
pub enum UpdateCommand {
    /// Show the running version and the newest available one (the default).
    Check,
    /// Install the available update on this computer and restart Mouser.
    Apply,
}

pub async fn run(mouser: &mut Mouser, action: Option<UpdateCommand>) -> Result<(), Failure> {
    let apply = matches!(action, Some(UpdateCommand::Apply));
    let method = if apply {
        Method::UpdateApply
    } else {
        Method::UpdateCheck
    };
    let value = mouser.call(method, json!({})).await?;
    let result: UpdateResult = decode(value.clone());
    mouser.print(&value, || {
        if apply {
            let to = result.available.expect("an applied update was available");
            format!("Updating from {} to {to}", result.current)
        } else {
            let available = result.available.as_deref().unwrap_or("none");
            format!(
                "Current version: {}\nAvailable version: {available}",
                result.current
            )
        }
    });
    Ok(())
}
