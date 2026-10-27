//! `logs enable|disable|path|tail`: diagnostic logging (FR-010, Story 9 sc. 3).

use std::path::PathBuf;

use base64::Engine as _;
use clap::Subcommand;
use mouser_core::logging;
use mouser_core::logs::bundle;
use mouser_core::paths::Env;
use mouser_proto::control::{
    BundleComputer, BundleReason, LoggingSetResult, LogsCollectResult, LogsCollectStreamResult,
    Method, Status,
};
use mouser_proto::wire::ComputerMode;
use serde_json::json;
use sha2::{Digest, Sha256};

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
    /// Gather the logs of every computer on the desk into one zip.
    Collect {
        /// The folder to write the zip to (default: your Downloads folder).
        #[arg(long)]
        out: Option<PathBuf>,
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
            let result: LoggingSetResult = crate::decode(value.clone())?;
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
        LogsCommand::Collect { out } => {
            let mut mouser = live.ok_or(Failure::NotRunning)?;
            collect(&mut mouser, json, out).await?;
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
            let status: Status = crate::decode(mouser.call(Method::Status, json!({})).await?)?;
            // 0.1.1 did not report its log.
            let logging = status.logging.ok_or(Failure::Outdated)?;
            Ok(PathBuf::from(logging.path))
        }
        None => Ok(crate::paths::dir().join("logs").join(logging::FILE)),
    }
}

/// The bundle, written by the running Mouser, or (the installed service
/// writes no user path, OD-7) streamed here and written as this user.
async fn collect(mouser: &mut Mouser, json: bool, out: Option<PathBuf>) -> Result<(), Failure> {
    let status: Status = crate::decode(mouser.call(Method::Status, json!({})).await?)?;
    let mode = status
        .computers
        .iter()
        .find(|c| c.id == status.computer.id)
        .and_then(|c| c.mode)
        .ok_or(Failure::Outdated)?;
    let result: LogsCollectResult = if mode == ComputerMode::Service {
        stream(mouser, out).await?
    } else {
        let params = json!({ "dir": out.map(|dir| dir.to_string_lossy().into_owned()) });
        crate::decode(mouser.call(Method::LogsCollect, params).await?)?
    };
    let value = serde_json::to_value(&result).expect("result serializes");
    emit(json, &value, || {
        std::iter::once(result.path.clone())
            .chain(result.computers.iter().map(line))
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(())
}

/// `logs.collect_stream`: reassemble the chunks, check them, write the zip.
async fn stream(mouser: &mut Mouser, out: Option<PathBuf>) -> Result<LogsCollectResult, Failure> {
    let streamed: LogsCollectStreamResult =
        crate::decode(mouser.call(Method::LogsCollectStream, json!({})).await?)?;
    let mut zip = Vec::new();
    for (seq, chunk) in mouser.client.take_chunks().into_iter().enumerate() {
        assert_eq!(chunk.seq as usize, seq, "log chunks arrive in order");
        zip.extend(
            base64::engine::general_purpose::STANDARD
                .decode(chunk.data)
                .expect("a chunk is base64"),
        );
    }
    assert_eq!(zip.len() as u64, streamed.size, "the bundle is whole");
    let hash: String = Sha256::digest(&zip)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(hash, streamed.sha256, "the bundle is intact");
    let dir = bundle::location(
        out.as_deref(),
        ComputerMode::Service,
        &crate::paths::dir(),
        &Env::from_process(),
    );
    let path = bundle::unique_path(&dir, &streamed.name);
    std::fs::create_dir_all(&dir).expect("the bundle folder can be created");
    std::fs::write(&path, &zip).expect("the bundle can be written");
    Ok(LogsCollectResult {
        path: path.to_string_lossy().into_owned(),
        computers: streamed.computers,
    })
}

/// `<name>: included` or `<name>: not included (<why>)`.
fn line(computer: &BundleComputer) -> String {
    let name = &computer.name;
    match computer.reason {
        None => format!("{name}: included"),
        Some(BundleReason::Offline) => format!("{name}: not included (offline)"),
        Some(BundleReason::LoggingOff) => format!("{name}: not included (logging off)"),
        Some(BundleReason::OlderVersion) => format!("{name}: not included (older version)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mouser_proto::ids::ComputerId;

    fn computer(reason: Option<BundleReason>) -> BundleComputer {
        BundleComputer {
            id: ComputerId([1; 16]),
            name: "Beta".into(),
            included: reason.is_none(),
            reason,
        }
    }

    #[test]
    fn each_computer_line_says_why_it_is_not_in_the_bundle() {
        let lines: Vec<String> = [
            None,
            Some(BundleReason::Offline),
            Some(BundleReason::LoggingOff),
            Some(BundleReason::OlderVersion),
        ]
        .map(|reason| line(&computer(reason)))
        .into();
        assert_eq!(
            lines,
            [
                "Beta: included",
                "Beta: not included (offline)",
                "Beta: not included (logging off)",
                "Beta: not included (older version)",
            ]
        );
    }
}
