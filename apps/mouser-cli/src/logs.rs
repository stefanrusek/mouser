//! `logs enable|disable|path|tail`: diagnostic logging (FR-010, Story 9 sc. 3).

use std::path::PathBuf;

use clap::Subcommand;
use mouser_core::config;
use mouser_core::logging;
use mouser_proto::control::{LoggingSetResult, Method, Status};
use serde_json::json;

use crate::{Failure, Mouser, emit};

#[derive(Subcommand)]
pub enum LogsCommand {
    /// Turn diagnostic logging on.
    Enable,
    /// Turn diagnostic logging off.
    Disable,
    /// Print where the log file is.
    Path,
    /// Print the last lines of the log as raw JSON Lines.
    Tail {
        /// How many lines.
        #[arg(short = 'n', default_value_t = 20)]
        lines: usize,
    },
}

pub async fn run(live: Option<Mouser>, json: bool, action: LogsCommand) -> Result<(), Failure> {
    match action {
        LogsCommand::Enable | LogsCommand::Disable => {
            let enabled = matches!(action, LogsCommand::Enable);
            let mut mouser = live.ok_or(Failure::NotRunning)?;
            let value = mouser
                .call(Method::LoggingSet, json!({ "enabled": enabled }))
                .await?;
            let result: LoggingSetResult = crate::decode(value.clone());
            emit(json, &value, || {
                if enabled {
                    format!("Logging is on: {}", result.path)
                } else {
                    "Logging is off".into()
                }
            });
        }
        LogsCommand::Path => {
            let path = path(live).await?;
            let shown = path.to_string_lossy().into_owned();
            emit(json, &json!({ "path": shown }), || shown.clone());
        }
        LogsCommand::Tail { lines } => {
            // No file yet means nothing has been logged.
            let text = std::fs::read_to_string(path(live).await?).unwrap_or_default();
            let all: Vec<&str> = text.lines().collect();
            for line in &all[all.len().saturating_sub(lines)..] {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// The running Mouser's log file, or the default place when it is stopped.
async fn path(live: Option<Mouser>) -> Result<PathBuf, Failure> {
    match live {
        Some(mut mouser) => {
            let status: Status = crate::decode(mouser.call(Method::Status, json!({})).await?);
            Ok(PathBuf::from(
                status.logging.expect("status reports logging").path,
            ))
        }
        None => Ok(config::default_dir().join("logs").join(logging::FILE)),
    }
}
