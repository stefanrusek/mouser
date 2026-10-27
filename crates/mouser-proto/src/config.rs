//! Configuration file (contracts/config.md).

use std::fmt;

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

/// Keys written locally that are propagated to the desk with a new epoch.
/// (Layout is desk-shared too but has no config key.)
pub const DESK_SHARED_KEYS: [&str; 5] = [
    "crossing.block_fullscreen",
    "crossing.equalize_dimensions",
    "advanced.allowlist",
    "crossing.landing_highlight",
    "keyboard.command_as_control",
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
#[serde(default)]
pub struct General {
    pub start_at_login: bool,
    pub notifications: bool,
    /// Empty = OS computer name.
    pub computer_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub enable_disable: String,
    pub quickjump: String,
    pub targeted_paste: String,
    pub panic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mouse {
    /// `"passthrough"` or `"back_forward"`.
    pub buttons_4_5: String,
}

/// Desk-shared; replicated via `DeskState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crossing {
    pub block_fullscreen: bool,
    pub equalize_dimensions: bool,
    pub landing_highlight: bool,
}

/// Desk-shared; replicated via `DeskSettings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keyboard {
    /// Mac Command acts as Control on Windows/Linux.
    pub command_as_control: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Diagnostics {
    pub logging: bool,
    pub max_log_mb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Updates {
    pub automatic: bool,
    pub release_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Advanced {
    /// UDP (QUIC) port, 1-65535.
    pub port: u32,
    /// CIDR strings; empty = off.
    pub allowlist: Vec<String>,
}

/// The whole config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema: u32,
    pub general: General,
    pub hotkeys: Hotkeys,
    pub mouse: Mouse,
    pub crossing: Crossing,
    pub keyboard: Keyboard,
    pub diagnostics: Diagnostics,
    pub updates: Updates,
    pub advanced: Advanced,
}

impl Default for General {
    fn default() -> Self {
        Self {
            start_at_login: true,
            notifications: true,
            computer_name: String::new(),
        }
    }
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            enable_disable: "Ctrl+Alt+Shift+M".into(),
            quickjump: "Ctrl+Alt+Shift+J".into(),
            targeted_paste: "Ctrl+Alt+Shift+V".into(),
            panic: "Escape*2".into(),
        }
    }
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            buttons_4_5: "passthrough".into(),
        }
    }
}

impl Default for Crossing {
    fn default() -> Self {
        Self {
            block_fullscreen: true,
            equalize_dimensions: false,
            landing_highlight: true,
        }
    }
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            command_as_control: true,
        }
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            logging: false,
            max_log_mb: 50,
        }
    }
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            automatic: true,
            release_url: "https://api.github.com/repos/stefanrusek/mouser/releases?per_page=10"
                .into(),
        }
    }
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            port: 47801,
            allowlist: Vec::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: 1,
            general: General::default(),
            hotkeys: Hotkeys::default(),
            mouse: Mouse::default(),
            crossing: Crossing::default(),
            keyboard: Keyboard::default(),
            diagnostics: Diagnostics::default(),
            updates: Updates::default(),
            advanced: Advanced::default(),
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
        if let Err(reason) = parse_chord(&self.hotkeys.enable_disable) {
            return Err(reject("hotkeys.enable_disable", reason));
        }
        if self.diagnostics.max_log_mb == 0 {
            return Err(reject("diagnostics.max_log_mb", "must be 1 or more".into()));
        }
        Ok(())
    }
}

/// The HID usage of a key name: letters, digits, F1-F12 and Escape, Space,
/// Enter, Tab, Backspace.
fn usage_for_name(name: &str) -> Option<u32> {
    const PAGE: u32 = 0x0007_0000;
    let fixed = match name {
        "Escape" => Some(0x29),
        "Space" => Some(0x2C),
        "Enter" => Some(0x28),
        "Tab" => Some(0x2B),
        "Backspace" => Some(0x2A),
        _ => None,
    };
    let single = name.chars().next().filter(|_| name.len() == 1);
    let id = match (fixed, single) {
        (Some(id), _) => id,
        (None, Some(c @ 'A'..='Z')) => 0x04 + (c as u32 - 'A' as u32),
        (None, Some(c @ '1'..='9')) => 0x1E + (c as u32 - '1' as u32),
        (None, Some('0')) => 0x27,
        _ => {
            let n: u32 = name.strip_prefix('F')?.parse().ok()?;
            if !(1..=12).contains(&n) {
                return None;
            }
            0x3A + (n - 1)
        }
    };
    Some(PAGE | id)
}

/// `"Ctrl+Alt+Shift+V"`: any of `Ctrl`, `Alt`, `Shift`, `Meta` (or `Cmd`),
/// then a key name. The same grammar `mouser-core` parses for `quickjump` and
/// `targeted_paste`.
fn parse_chord(text: &str) -> Result<(), String> {
    let mut names: Vec<&str> = text.split('+').collect();
    let key = names.pop().expect("split yields at least one part");
    usage_for_name(key).ok_or_else(|| format!("{key:?} is not a known key"))?;
    for name in names {
        if !matches!(name, "Ctrl" | "Alt" | "Shift" | "Meta" | "Cmd") {
            return Err(format!(
                "{name:?} is not a modifier (Ctrl, Alt, Shift, Meta)"
            ));
        }
    }
    Ok(())
}
