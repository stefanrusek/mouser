//! How this run of Mouser is deployed (data-model "mode").

use mouser_proto::wire::ComputerMode;

use crate::headless::is_headless;

/// The mode for a launch with `args`: `--headless --machine` is the service,
/// `--headless` the login-window agent, `MouserData/` beside the executable
/// (`portable`) a portable run, elevated or not, and anything else an install.
pub fn select(args: &[String], portable: bool, elevated: bool) -> ComputerMode {
    if is_headless(args) {
        if args.iter().any(|a| a == "--machine") {
            ComputerMode::Service
        } else {
            ComputerMode::Headless
        }
    } else if portable {
        if elevated {
            ComputerMode::PortableElevated
        } else {
            ComputerMode::Portable
        }
    } else {
        ComputerMode::Installed
    }
}
