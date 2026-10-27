//! The Mouser engine: identity, desk membership, discovery, transport, control API.

pub mod autostart;
pub mod clipboard;
pub mod config;
pub mod control;
pub mod crossing;
pub mod desk;
pub mod discovery;
pub mod engine;
pub mod engine_headless;
pub mod faults;
pub mod identity;
pub mod layout;
pub(crate) mod lock;
pub mod logging;
pub mod logs;
pub mod paths;
pub mod portable;
pub mod problems;
pub mod quickjump;
pub mod release;
pub mod session;
pub mod state;
pub mod transfer;
pub mod transport;

#[cfg(test)]
mod engine_headless_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod faults_tests;
#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod paths_tests;
#[cfg(test)]
mod portable_tests;
#[cfg(test)]
mod testing;

pub use control::{ControlHandle, client::ControlClient};
pub use engine::{Engine, EngineEvent, EngineParams};

/// The wire/control-API name of a platform's operating system.
pub(crate) fn platform_os(os: mouser_platform::Os) -> mouser_proto::model::Os {
    match os {
        mouser_platform::Os::MacOs => mouser_proto::model::Os::Macos,
        mouser_platform::Os::Windows => mouser_proto::model::Os::Windows,
        mouser_platform::Os::Linux => mouser_proto::model::Os::Linux,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_os_has_a_wire_name() {
        use mouser_platform::Os as P;
        use mouser_proto::model::Os as W;
        assert_eq!(platform_os(P::MacOs), W::Macos);
        assert_eq!(platform_os(P::Windows), W::Windows);
        assert_eq!(platform_os(P::Linux), W::Linux);
    }
}
