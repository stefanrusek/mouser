//! The diagnostic log (FR-010): one JSON record per line in a file under the
//! config directory, written only while the owner has logging on. Nothing
//! goes to the console or the system log. The file rotates at
//! `diagnostics.max_log_mb`: the full file becomes `mouser.log.1`, replacing
//! the previous one, and a new `mouser.log` starts.

use crate::lock::LockExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mouser_proto::control::{LoggingSetResult, RpcError};
use mouser_proto::ids::ComputerId;
use mouser_proto::log::{LogComputer, LogLevel, LogRecord};

use crate::engine::Ctx;

/// File name inside `<config dir>/logs`.
pub const FILE: &str = "mouser.log";

struct Inner {
    enabled: bool,
    name: String,
    max_bytes: u64,
    file: Option<File>,
    size: u64,
}

pub struct Log {
    computer: ComputerId,
    path: PathBuf,
    inner: Mutex<Inner>,
}

fn megabytes(mb: u32) -> u64 {
    u64::from(mb) * 1024 * 1024
}

impl Log {
    pub fn new(
        config_dir: &Path,
        computer: ComputerId,
        name: String,
        enabled: bool,
        max_mb: u32,
    ) -> Self {
        let log = Self {
            computer,
            path: config_dir.join("logs").join(FILE),
            inner: Mutex::new(Inner {
                enabled,
                name,
                max_bytes: megabytes(max_mb),
                file: None,
                size: 0,
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

    pub fn set_max_bytes(&self, bytes: u64) {
        self.inner.guard().max_bytes = bytes;
    }

    fn open(&self, inner: &mut Inner) -> std::io::Result<()> {
        inner.file = None;
        std::fs::create_dir_all(self.path.parent().expect("log has a directory"))?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        inner.size = file.metadata()?.len();
        inner.file = Some(file);
        Ok(())
    }

    /// The config changed: follow its logging switch, size limit and the
    /// computer's name. Turning logging on or off is itself recorded.
    pub fn apply(&self, enabled: bool, max_mb: u32, name: &str) {
        let was = {
            let mut inner = self.inner.guard();
            inner.max_bytes = megabytes(max_mb);
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
            self.record(LogLevel::Info, "logging.enabled", None, None);
        } else {
            self.record(LogLevel::Info, "logging.disabled", None, None);
            let mut inner = self.inner.guard();
            inner.enabled = false;
            inner.file = None;
        }
    }

    /// Append one record (nothing while logging is off).
    pub fn record(
        &self,
        level: LogLevel,
        event: &str,
        peer: Option<ComputerId>,
        fields: Option<serde_json::Map<String, serde_json::Value>>,
    ) {
        let mut inner = self.inner.guard();
        if !inner.enabled {
            return;
        }
        let mut line = LogRecord {
            ts: crate::problems::now(),
            level,
            computer: LogComputer {
                id: self.computer,
                name: inner.name.clone(),
            },
            event: event.to_string(),
            peer,
            fields,
        }
        .to_line()
        .expect("log events are named in dotted lowercase");
        line.push('\n');
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
        // A file that is not open or not writable drops the record.
        let Some(file) = inner.file.as_mut() else {
            return;
        };
        let _ = file.write_all(line.as_bytes());
        inner.size += len;
    }
}

impl Ctx {
    /// `logging.set`: the checkbox. It is the config's `diagnostics.logging`,
    /// so it persists and the UI hears of it through `status`.
    pub(crate) fn set_logging(
        self: &Arc<Self>,
        enabled: bool,
    ) -> Result<LoggingSetResult, RpcError> {
        self.set_config("diagnostics.logging", enabled.into())?;
        Ok(LoggingSetResult {
            path: self.log.path().to_string_lossy().into_owned(),
        })
    }
}

#[cfg(test)]
mod tests;
