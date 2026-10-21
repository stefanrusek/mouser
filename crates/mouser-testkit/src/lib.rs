//! Sanctioned test doubles (decision D2): `VirtualPlatform` and `SimNet`.

pub mod report;
pub mod simnet;
pub mod virtual_platform;

pub use simnet::{SimNet, SimNode};
pub use virtual_platform::{Injected, VirtualPlatform};
