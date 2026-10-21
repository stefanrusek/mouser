//! Configuration file (contracts/config.md).

use std::fmt;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// Keys written locally that are propagated to the desk with a new epoch.
/// (Layout is desk-shared too but has no config key.)
pub const DESK_SHARED_KEYS: [&str; 3] = [
    "crossing.block_fullscreen",
    "crossing.equalize_dimensions",
    "advanced.allowlist",
];

/// A rejected value, rendered `key: reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub key: String,
    pub reason: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.key, self.reason)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct General {
    pub start_at_login: bool,
    pub notifications: bool,
    /// Empty = OS computer name.
    pub computer_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkeys {
    pub enable_disable: String,
    pub quickjump: String,
    pub targeted_paste: String,
    pub panic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mouse {
    /// `"passthrough"` or `"back_forward"`.
    pub buttons_4_5: String,
}

/// Desk-shared; replicated via `DeskState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crossing {
    pub block_fullscreen: bool,
    pub equalize_dimensions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostics {
    pub logging: bool,
    pub max_log_mb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Updates {
    pub automatic: bool,
    pub release_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advanced {
    /// UDP (QUIC) port, 1-65535.
    pub port: u32,
    /// CIDR strings; empty = off.
    pub allowlist: Vec<String>,
}

/// The whole config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub schema: u32,
    pub general: General,
    pub hotkeys: Hotkeys,
    pub mouse: Mouse,
    pub crossing: Crossing,
    pub diagnostics: Diagnostics,
    pub updates: Updates,
    pub advanced: Advanced,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            general: General {
                start_at_login: true,
                notifications: true,
                computer_name: String::new(),
            },
            hotkeys: Hotkeys {
                enable_disable: "Ctrl+Alt+Shift+M".into(),
                quickjump: "Ctrl+Alt+Shift+J".into(),
                targeted_paste: "Ctrl+Alt+Shift+V".into(),
                panic: "Escape*2".into(),
            },
            mouse: Mouse {
                buttons_4_5: "passthrough".into(),
            },
            crossing: Crossing {
                block_fullscreen: true,
                equalize_dimensions: false,
            },
            diagnostics: Diagnostics {
                logging: false,
                max_log_mb: 50,
            },
            updates: Updates {
                automatic: true,
                release_url: "https://api.github.com/repos/<owner>/mouser/releases/latest".into(),
            },
            advanced: Advanced {
                port: 47801,
                allowlist: Vec::new(),
            },
        }
    }
}

fn reject(key: &str, reason: String) -> ConfigError {
    ConfigError {
        key: key.into(),
        reason,
    }
}

impl Config {
    /// Check the value constraints; the first violation is reported as `key: reason`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !matches!(
            self.mouse.buttons_4_5.as_str(),
            "passthrough" | "back_forward"
        ) {
            return Err(reject(
                "mouse.buttons_4_5",
                "must be \"passthrough\" or \"back_forward\"".into(),
            ));
        }
        if !(1..=65535).contains(&self.advanced.port) {
            return Err(reject(
                "advanced.port",
                "must be between 1 and 65535".into(),
            ));
        }
        if let Some(bad) = self
            .advanced
            .allowlist
            .iter()
            .find(|e| e.parse::<IpNet>().is_err())
        {
            return Err(reject(
                "advanced.allowlist",
                format!("{bad:?} is not a CIDR network"),
            ));
        }
        Ok(())
    }
}
