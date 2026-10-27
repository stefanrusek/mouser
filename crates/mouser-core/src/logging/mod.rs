//! The diagnostic log (FR-010): one JSON record per line in files under the
//! config directory, written only while the owner has logging on. Nothing
//! goes to the console or the system log. Two files share the limit
//! `diagnostics.max_log_mb`: `mouser.log` rotates to `mouser.log.1`, replacing
//! the previous one, when the next record would take it past half of the
//! limit, and every file opened starts with `log.header` (FR-008). Records are
//! built only by the constructors of [`events`].

pub mod events;

#[cfg(test)]
pub(crate) mod events_tests;
#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod rotation_tests;
#[cfg(test)]
mod tests;

use crate::lock::LockExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mouser_proto::control::{LoggingSetResult, RpcError, SettingOrigin};
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::log::{LogComputer, LogLevel};
use mouser_proto::model::Os;
use mouser_proto::wire::ComputerMode;
use serde_json::{Map, Value};

use crate::engine::Ctx;

/// File name inside `<config dir>/logs`.
pub const FILE: &str = "mouser.log";

#[cfg(target_os = "macos")]
const BUILT_FOR: Os = Os::Macos;
#[cfg(windows)]
const BUILT_FOR: Os = Os::Windows;
#[cfg(not(any(target_os = "macos", windows)))]
const BUILT_FOR: Os = Os::Linux;

/// What `log.header` says about this computer besides its id and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub mouser_version: String,
    pub os: Os,
    pub os_version: String,
    pub mode: ComputerMode,
    /// The desk this computer is on, if any.
    pub desk: Option<(DeskId, String)>,
}

impl Context {
    /// This build, for a log whose host says nothing more: the version of
    /// the crate, the operating system it was built for.
    pub fn fallback() -> Self {
        Self {
            mouser_version: env!("CARGO_PKG_VERSION").to_string(),
            os: BUILT_FOR,
            os_version: "unknown".to_string(),
            mode: ComputerMode::Installed,
            desk: None,
        }
    }
}

struct Inner {
    enabled: bool,
    name: String,
    /// The most one file may hold: half of `diagnostics.max_log_mb`.
    max_bytes: u64,
    file: Option<File>,
    size: u64,
    context: Context,
}

pub struct Log {
    computer: ComputerId,
    path: PathBuf,
    inner: Mutex<Inner>,
}

/// The size one file may reach: half of the limit, so two files stay within it.
fn per_file(mb: u32) -> u64 {
    u64::from(mb) * 1024 * 1024 / 2
}

impl Log {
    /// A log for a host that says nothing about itself beyond [`Context::fallback`].
    pub fn new(
        config_dir: &Path,
        computer: ComputerId,
        name: String,
        enabled: bool,
        max_mb: u32,
    ) -> Self {
        Self::with_context(
            config_dir,
            computer,
            name,
            enabled,
            max_mb,
            Context::fallback(),
        )
    }

    pub fn with_context(
        config_dir: &Path,
        computer: ComputerId,
        name: String,
        enabled: bool,
        max_mb: u32,
        context: Context,
    ) -> Self {
        let log = Self {
            computer,
            path: config_dir.join("logs").join(FILE),
            inner: Mutex::new(Inner {
                enabled,
                name,
                max_bytes: per_file(max_mb),
                file: None,
                size: 0,
                context,
            }),
        };
        if enabled {
            // A file that cannot be opened drops the records: there is
            // nowhere to report it.
            let _ = log.open(&mut log.inner.guard());
        }
        log
    }

    /// Where the log is (whether or not logging is on).
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn enabled(&self) -> bool {
        self.inner.guard().enabled
    }

    /// The desk this computer is on now, for the header of the next file.
    pub fn set_desk(&self, desk: Option<(DeskId, String)>) {
        self.inner.guard().context.desk = desk;
    }

    /// One line of the log: `event` as a record of this computer, now.
    fn line(&self, inner: &Inner, event: events::Event) -> String {
        let mut line = event
            .into_record(
                LogComputer {
                    id: self.computer,
                    name: inner.name.clone(),
                },
                crate::problems::now(),
            )
            .to_line()
            .expect("log events are named in dotted lowercase");
        line.push('\n');
        line
    }

    /// Open the file for appending and start this run of it with `log.header`.
    fn open(&self, inner: &mut Inner) -> std::io::Result<()> {
        inner.file = None;
        std::fs::create_dir_all(self.path.parent().expect("log has a directory"))?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        inner.size = file.metadata()?.len();
        inner.file = Some(file);
        let header = self.line(
            inner,
            events::header(self.computer, &inner.name, &inner.context, &self.path),
        );
        self.append(inner, &header);
        Ok(())
    }

    /// Write `line` to the open file; a file that is not open or not
    /// writable drops it.
    fn append(&self, inner: &mut Inner, line: &str) {
        let Some(file) = inner.file.as_mut() else {
            return;
        };
        let _ = file.write_all(line.as_bytes());
        inner.size += line.len() as u64;
    }

    /// The config changed: follow its logging switch, size limit and the
    /// computer's name. Turning logging on or off is itself recorded.
    pub fn apply(&self, enabled: bool, max_mb: u32, name: &str) {
        let was = {
            let mut inner = self.inner.guard();
            inner.max_bytes = per_file(max_mb);
            inner.name = name.to_string();
            inner.enabled
        };
        if enabled == was {
            return;
        }
        if enabled {
            let mut inner = self.inner.guard();
            let _ = self.open(&mut inner);
            inner.enabled = true;
            drop(inner);
            self.write(events::logging_enabled(&self.path));
        } else {
            self.write(events::logging_disabled(&self.path));
            let mut inner = self.inner.guard();
            inner.enabled = false;
            inner.file = None;
        }
    }

    /// Append one record (nothing while logging is off).
    pub fn write(&self, event: events::Event) {
        let mut inner = self.inner.guard();
        if !inner.enabled {
            return;
        }
        let line = self.line(&inner, event);
        let len = line.len() as u64;
        if inner.size > 0 && inner.size + len > inner.max_bytes {
            // Windows cannot rename over an existing file.
            let rotated = self.path.with_extension("log.1");
            let _ = std::fs::remove_file(&rotated);
            if std::fs::rename(&self.path, &rotated).is_err() {
                return;
            }
            let _ = self.open(&mut inner);
        }
        self.append(&mut inner, &line);
    }

    /// The bytes of the log files, `mouser.log` and then `mouser.log.1`
    /// where it exists, as they are at one moment: nothing is written (and
    /// nothing rotates) while they are read.
    pub fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
        let _quiet = self.inner.guard();
        [
            (FILE, self.path.clone()),
            ("mouser.log.1", self.path.with_extension("log.1")),
        ]
        .into_iter()
        .filter_map(|(name, path)| Some((name.to_string(), std::fs::read(path).ok()?)))
        .collect()
    }

    /// What a platform backend reports through its diagnostics sink: a
    /// `capture.*` event of the catalog.
    pub fn record(
        &self,
        level: LogLevel,
        event: &str,
        peer: Option<ComputerId>,
        fields: Option<Map<String, Value>>,
    ) {
        self.write(events::platform_diagnostic(level, event, peer, fields));
    }
}

impl Ctx {
    /// `logging.set`: the checkbox. It is the config's `diagnostics.logging`,
    /// so it persists and the UI hears of it through `status`.
    pub(crate) fn set_logging(
        self: &Arc<Self>,
        enabled: bool,
    ) -> Result<LoggingSetResult, RpcError> {
        self.set_config("diagnostics.logging", enabled.into(), SettingOrigin::App)?;
        Ok(LoggingSetResult {
            path: self.log.path().to_string_lossy().into_owned(),
        })
    }
}
