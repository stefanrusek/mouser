//! The bundle file (contracts/log-bundle.md): its name, where it goes, and
//! the zip of every computer's logs. Nothing leaves the requesting computer
//! (FR-007), and only the files the contract names go into the zip, whatever
//! a desk-mate says it sent.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Utc};
use mouser_proto::control::{BundleComputer, BundleReason, ConnectionState};
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::Os;
use mouser_proto::wire::ComputerMode;
use serde::Serialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::paths::Env;

/// The only files a computer's folder in the zip holds besides
/// `computer.json` and `config.toml`.
const LOG_FILES: [&str; 4] = ["mouser.log", "mouser.log.1", "service.log", "service.log.1"];

/// The settings file, in the folder and in the manifest.
const SETTINGS: &str = "config.toml";

/// One computer of the bundle, as the requester knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Computer {
    pub id: ComputerId,
    pub name: String,
    pub os: Os,
    pub os_version: Option<String>,
    pub version: String,
    pub mode: Option<ComputerMode>,
    pub connection: ConnectionState,
    pub included: bool,
    pub reason: Option<BundleReason>,
    /// `config.toml`, verbatim.
    pub config_toml: Option<String>,
    /// `(file name, bytes)` of the log files it sent.
    pub files: Vec<(String, Vec<u8>)>,
}

/// `computer.json`: the card as the requester sees it.
#[derive(Serialize)]
struct Card<'a> {
    id: ComputerId,
    name: &'a str,
    os: Os,
    #[serde(skip_serializing_if = "Option::is_none")]
    os_version: Option<&'a str>,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<ComputerMode>,
    connection: ConnectionState,
}

/// One computer in `bundle.json`.
#[derive(Serialize)]
struct Entry<'a> {
    #[serde(flatten)]
    card: Card<'a>,
    included: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<BundleReason>,
    folder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings_file: Option<&'a str>,
    files: Vec<&'a str>,
}

#[derive(Serialize)]
struct DeskEntry<'a> {
    id: DeskId,
    name: &'a str,
}

#[derive(Serialize)]
struct Manifest<'a> {
    created_at: String,
    requested_by: ComputerId,
    desk: Option<DeskEntry<'a>>,
    computers: Vec<Entry<'a>>,
}

/// `name` with every character outside `[A-Za-z0-9_-]` replaced by `-`.
fn safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// `mouser-logs-<desk>-<yyyymmddThhmmssZ>.zip`; `no-desk` for a computer
/// on no desk.
pub fn file_name(desk: Option<&str>, at: DateTime<Utc>) -> String {
    format!(
        "mouser-logs-{}-{}.zip",
        desk.map_or_else(|| "no-desk".to_string(), safe),
        at.format("%Y%m%dT%H%M%SZ")
    )
}

/// The folder the bundle goes to: the one asked for, else the machine
/// folder's `bundles` for a headless engine on macOS or Linux, else the
/// user's Downloads folder.
pub fn location(dir: Option<&Path>, mode: ComputerMode, machine_dir: &Path, env: &Env) -> PathBuf {
    match dir {
        Some(dir) => dir.to_path_buf(),
        None if mode == ComputerMode::Headless && !cfg!(windows) => machine_dir.join("bundles"),
        None => env
            .home
            .clone()
            .expect("the environment names the user's folders")
            .join("Downloads"),
    }
}

/// `name` in `dir`, or ` (2)`, ` (3)` and so on added before `.zip` when
/// that name is taken.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.strip_suffix(".zip").expect("a bundle is named .zip");
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).zip")))
        .find(|path| !path.exists())
        .expect("an endless range")
}

/// The folder of a computer in the zip.
fn folder(computer: &Computer) -> String {
    format!("{}-{}", safe(&computer.name), &computer.id.to_string()[..8])
}

/// The files of `computer` that belong in the zip.
fn kept(computer: &Computer) -> Vec<&(String, Vec<u8>)> {
    computer
        .files
        .iter()
        .filter(|(name, _)| LOG_FILES.contains(&name.as_str()))
        .collect()
}

fn card(computer: &Computer) -> Card<'_> {
    Card {
        id: computer.id,
        name: &computer.name,
        os: computer.os,
        os_version: computer.os_version.as_deref(),
        version: &computer.version,
        mode: computer.mode,
        connection: computer.connection,
    }
}

/// The zip, in memory. `computers` has the requester first.
pub fn build(
    at: DateTime<Utc>,
    requested_by: ComputerId,
    desk: Option<(DeskId, &str)>,
    computers: &[Computer],
) -> Vec<u8> {
    let manifest = Manifest {
        created_at: at.to_rfc3339_opts(SecondsFormat::Secs, true),
        requested_by,
        desk: desk.map(|(id, name)| DeskEntry { id, name }),
        computers: computers
            .iter()
            .map(|c| Entry {
                card: card(c),
                included: c.included,
                reason: c.reason,
                folder: folder(c),
                settings_file: c.config_toml.as_ref().map(|_| SETTINGS),
                files: kept(c).iter().map(|(name, _)| name.as_str()).collect(),
            })
            .collect(),
    };
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut add = |name: &str, bytes: &[u8]| {
        zip.start_file(name, options).expect("zip starts a file");
        zip.write_all(bytes).expect("zip writes to memory");
    };
    add(
        "bundle.json",
        &serde_json::to_vec_pretty(&manifest).expect("manifest serializes"),
    );
    for computer in computers {
        let folder = folder(computer);
        add(
            &format!("{folder}/computer.json"),
            &serde_json::to_vec_pretty(&card(computer)).expect("card serializes"),
        );
        if let Some(config) = &computer.config_toml {
            add(&format!("{folder}/{SETTINGS}"), config.as_bytes());
        }
        for (name, bytes) in kept(computer) {
            add(&format!("{folder}/{name}"), bytes);
        }
    }
    zip.finish().expect("zip finishes in memory").into_inner()
}

/// Who is in the bundle, as `logs.collect` answers.
pub fn summary(computers: &[Computer]) -> Vec<BundleComputer> {
    computers
        .iter()
        .map(|c| BundleComputer {
            id: c.id,
            name: c.name.clone(),
            included: c.included,
            reason: c.reason,
        })
        .collect()
}
