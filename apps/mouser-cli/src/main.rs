//! The `mouser` command-line client of a running Mouser (contracts/cli.md).

mod desk;
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
        if self.json {
            println!("{value}");
        } else {
            println!("{}", text());
        }
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
    /// Stop crossing on every computer.
    Pause,
    /// Resume crossing on every computer.
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
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let mut mouser = Mouser::connect(cli.json).await?;
    match cli.command {
        Command::Status => status::run(&mut mouser).await,
        Command::Pause => pause::run(&mut mouser, true).await,
        Command::Resume => pause::run(&mut mouser, false).await,
        Command::Desks => desk::desks(&mut mouser).await,
        Command::Desk { action } => desk::run(&mut mouser, action).await,
        Command::Requests => desk::requests(&mut mouser).await,
        Command::Approve { request } => desk::decide(&mut mouser, request, true).await,
        Command::Decline { request } => desk::decide(&mut mouser, request, false).await,
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
