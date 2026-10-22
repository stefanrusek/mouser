//! The `mouser` command-line client of a running Mouser (contracts/cli.md).

mod config;
mod desk;
mod logs;
mod pause;
mod status;

use std::fmt;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mouser_core::ControlClient;
use mouser_core::control::default_path;
use mouser_proto::control::{Method, RpcError};
use serde_json::Value;

/// Why a command did not succeed; decides the exit code.
#[derive(Debug)]
pub enum Failure {
    /// No Mouser is serving the control socket (exit 2).
    NotRunning,
    /// An argument or value cannot be used (exit 3).
    Invalid(String),
    /// Mouser answered with an error (exit 1, or 3 for invalid params).
    Rpc(RpcError),
    /// The desk declined the join (exit 1).
    Declined(String),
}

/// JSON-RPC "invalid params": the engine rejected an argument.
const INVALID_PARAMS: i64 = -32602;

impl Failure {
    fn exit_code(&self) -> u8 {
        match self {
            Self::NotRunning => 2,
            Self::Invalid(_) => 3,
            Self::Rpc(error) if error.code == INVALID_PARAMS => 3,
            Self::Rpc(_) | Self::Declined(_) => 1,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => f.write_str("Mouser is not running"),
            Self::Invalid(message) | Self::Declined(message) => f.write_str(message),
            Self::Rpc(error) => f.write_str(&error.message),
        }
    }
}

/// Decode a control-API result.
pub fn decode<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("Mouser answers with the documented shape")
}

/// One connection to the running Mouser's control API.
pub struct Mouser {
    client: ControlClient,
    /// Print machine-readable JSON instead of text.
    pub json: bool,
}

impl Mouser {
    async fn connect(json: bool) -> Result<Self, Failure> {
        let client = ControlClient::connect(&default_path())
            .await
            .map_err(|_| Failure::NotRunning)?;
        Ok(Self { client, json })
    }

    /// Call a control-API method.
    pub async fn call(&mut self, method: Method, params: Value) -> Result<Value, Failure> {
        self.client
            .call(method, Some(params))
            .await
            .map_err(Failure::Rpc)
    }

    /// Print `value` as JSON, or `text` when not in `--json` mode.
    pub fn print(&self, value: &Value, text: impl FnOnce() -> String) {
        emit(self.json, value, text);
    }
}

/// Print `value` as JSON when `json`, otherwise `text`.
pub fn emit(json: bool, value: &Value, text: impl FnOnce() -> String) {
    if json {
        println!("{value}");
    } else {
        println!("{}", text());
    }
}

#[derive(Parser)]
#[command(name = "mouser", version, about = "Control a running Mouser")]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Desk, computers, connections, missing permissions and problems.
    Status,
    /// Stop crossing on this computer only (run it on each computer to pause the desk).
    Pause,
    /// Resume crossing on this computer only.
    Resume,
    /// Desks discovered on the network.
    Desks,
    /// Create, join or leave a desk, or remove a computer from it.
    Desk {
        #[command(subcommand)]
        action: desk::DeskCommand,
    },
    /// Pending join requests.
    Requests,
    /// Allow a computer to join this computer's desk.
    Approve {
        /// The request number from `mouser requests`.
        request: u64,
    },
    /// Decline a computer's request to join this computer's desk.
    Decline {
        /// The request number from `mouser requests`.
        request: u64,
    },
    /// Read or change settings (works on the file when Mouser is not running).
    Config {
        #[command(subcommand)]
        action: config::ConfigCommand,
    },
    /// Diagnostic logging (path and tail work when Mouser is not running).
    Logs {
        #[command(subcommand)]
        action: logs::LogsCommand,
    },
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let json = cli.json;
    // Only config and logs can do without a running Mouser.
    let connection = Mouser::connect(json).await;
    match cli.command {
        Command::Config { action } => config::run(connection.ok(), json, action).await,
        Command::Logs { action } => logs::run(connection.ok(), json, action).await,
        Command::Status => status::run(&mut connection?).await,
        Command::Pause => pause::run(&mut connection?, true).await,
        Command::Resume => pause::run(&mut connection?, false).await,
        Command::Desks => desk::desks(&mut connection?).await,
        Command::Desk { action } => desk::run(&mut connection?, action).await,
        Command::Requests => desk::requests(&mut connection?).await,
        Command::Approve { request } => desk::decide(&mut connection?, request, true).await,
        Command::Decline { request } => desk::decide(&mut connection?, request, false).await,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            error.print().expect("usage text printable");
            return ExitCode::from(if error.use_stderr() { 3 } else { 0 });
        }
    };
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("{failure}");
            ExitCode::from(failure.exit_code())
        }
    }
}
