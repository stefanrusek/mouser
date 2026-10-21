//! Release manifest: the tauri-plugin-updater `latest.json` format.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One platform bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAsset {
    pub url: String,
    /// minisign signature (base64 `.sig` content).
    pub signature: String,
}

/// `latest.json`. JSON only; the wire carries it as a JSON string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pub_date: Option<String>,
    /// Lowest peer protocol version this release interoperates with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_protocol: Option<u16>,
    /// `"<os>-<arch>"` to asset.
    pub platforms: BTreeMap<String, PlatformAsset>,
}
