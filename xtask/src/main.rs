mod bootstrap;
mod feature_dir;
mod gate;

use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install cargo-llvm-cov and UI dependencies.
    Bootstrap,
    /// Run the quality gate and append evidence records.
    Gate(gate::Args),
}

fn main() {
    match Cli::parse().command {
        Command::Bootstrap => bootstrap::run(),
        Command::Gate(args) => std::process::exit(gate::run(args)),
    }
}
