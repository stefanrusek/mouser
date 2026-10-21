//! Domain model types shared by wire, control API and config (data-model.md).

use serde::{Deserialize, Serialize};

use crate::ids::{ComputerId, DevicePublicKey};

/// Operating system family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

/// Side (edge) of a display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Platform permission kinds (data-model.md Computer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Accessibility,
    InputMonitoring,
    ScreenRecording,
    FullDiskAccess,
    InputCapturePortal,
    RemoteDesktopPortal,
}

/// State of one permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Missing,
    NotApplicable,
}

/// `(ComputerId, platform display key)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DisplayId {
    pub computer: ComputerId,
    pub key: String,
}

/// One physical display of a computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Display {
    pub display_id: DisplayId,
    pub name: String,
    pub pixel_size: (u32, u32),
    /// `pixel_size / scale`.
    pub logical_size: (f32, f32),
    /// Always `>= 1.0`.
    pub scale: f32,
    pub physical_size_mm: Option<(f32, f32)>,
    /// Position in the owning computer's own desktop coordinates.
    pub origin_local: (i32, i32),
    /// `false` = removed from the layout.
    pub included: bool,
}

/// Position of one display in desk space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub display_id: DisplayId,
    pub x: i32,
    pub y: i32,
}

/// Per-(display, side) edge options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeOptions {
    pub display_id: DisplayId,
    pub side: Side,
    /// Defaults to `true`.
    pub block_corners: bool,
}

impl EdgeOptions {
    /// Edge options with the default (`block_corners = true`).
    pub fn new(display_id: DisplayId, side: Side) -> Self {
        Self {
            display_id,
            side,
            block_corners: true,
        }
    }
}

/// Desk layout (exactly one per desk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub placements: Vec<Placement>,
    pub edge_options: Vec<EdgeOptions>,
    /// Defaults to `false`.
    pub equalize_dimensions: bool,
    /// Defaults to `true`.
    pub block_fullscreen: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            placements: Vec::new(),
            edge_options: Vec::new(),
            equalize_dimensions: false,
            block_fullscreen: true,
        }
    }
}

/// A desk member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub computer_id: ComputerId,
    pub device_public_key: DevicePublicKey,
    pub display_name: String,
    pub os: Os,
    pub mouser_version: String,
    /// RFC 3339 UTC timestamp.
    pub joined_at: String,
}

/// Replicated desk state; highest `(epoch, computer_id)` wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskState {
    pub epoch: u64,
    /// The member that produced this epoch.
    pub author: ComputerId,
    pub members: Vec<Member>,
    pub layout: Layout,
    /// CIDR strings; empty = off.
    pub allowlist: Vec<String>,
}

impl DeskState {
    /// True when `self` wins over `other`: higher `(epoch, author)`, lexicographically.
    pub fn supersedes(&self, other: &DeskState) -> bool {
        (self.epoch, self.author) > (other.epoch, other.author)
    }
}
