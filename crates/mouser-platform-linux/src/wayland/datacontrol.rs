//! The clipboard through `ext-data-control` (or `wlr-data-control`) when the
//! portal's clipboard is not part of the grant: KDE Plasma offers it to
//! privileged clients without a dialog. GNOME does not implement it.
//!
//! wl-clipboard-rs reads the Wayland socket from the process environment.

use crate::clipboard::{WAYLAND_TEXT, decode, encode, mime_types};
use mouser_platform::{ClipRep, Error, Result};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use wl_clipboard_rs::copy::{MimeSource, MimeType as CopyMime, Options, Source};
use wl_clipboard_rs::paste::{
    ClipboardType, Error as PasteError, MimeType, Seat, get_contents, get_mime_types_ordered,
};
use wl_clipboard_rs::watch::{CancelHandle, ClipboardType as Watched, Watcher};

/// How long a write waits for the compositor to announce the new selection.
const ANNOUNCED_WITHIN: Duration = Duration::from_secs(10);

fn os(error: impl std::fmt::Display) -> Error {
    Error::Os(error.to_string())
}

/// Selection changes seen so far, and a way to wait for the next one.
#[derive(Default)]
struct Changes {
    seen: Mutex<u64>,
    changed: Condvar,
}

impl Changes {
    fn bump(&self) {
        *self.seen.lock().unwrap() += 1;
        self.changed.notify_all();
    }

    fn count(&self) -> u64 {
        *self.seen.lock().unwrap()
    }

    fn wait_past(&self, count: u64) {
        let seen = self.seen.lock().unwrap();
        let (_seen, timeout) = self
            .changed
            .wait_timeout_while(seen, ANNOUNCED_WITHIN, |seen| *seen <= count)
            .unwrap();
        assert!(
            !timeout.timed_out(),
            "the compositor did not announce the new selection"
        );
    }
}

pub(crate) struct DataControl {
    changes: Arc<Changes>,
    cancel: CancelHandle,
    watcher: Option<JoinHandle<()>>,
}

impl DataControl {
    /// Connects to the compositor and starts following its selection. The
    /// first event is the selection as it is now.
    pub(crate) fn start() -> Result<DataControl> {
        let mut watcher = Watcher::new(Watched::Regular, Seat::Unspecified).map_err(os)?;
        let cancel = watcher.cancel_handle();
        let changes = Arc::new(Changes::default());
        watcher.next_event().map_err(os)?;
        changes.bump();
        let counter = changes.clone();
        let watcher = std::thread::spawn(move || {
            // Ends when cancelled, or when the compositor goes away.
            while let Ok(Some(_)) = watcher.next_event() {
                counter.bump();
            }
        });
        Ok(DataControl {
            changes,
            cancel,
            watcher: Some(watcher),
        })
    }

    pub(crate) fn change_count(&self) -> u64 {
        self.changes.count()
    }

    pub(crate) fn read(&self) -> Result<Vec<ClipRep>> {
        let offered = match get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified) {
            Ok(offered) => offered,
            Err(PasteError::ClipboardEmpty) => return Ok(Vec::new()),
            Err(other) => return Err(os(other)),
        };
        let mut items = HashMap::new();
        for mime in mime_types(WAYLAND_TEXT) {
            if offered.iter().any(|o| o == mime) {
                let (mut pipe, _) = get_contents(
                    ClipboardType::Regular,
                    Seat::Unspecified,
                    MimeType::Specific(mime),
                )
                .map_err(os)?;
                let mut data = Vec::new();
                pipe.read_to_end(&mut data).map_err(os)?;
                items.insert(mime.to_string(), data);
            }
        }
        decode(WAYLAND_TEXT, |mime| {
            Ok::<_, Error>(items.get(mime).cloned())
        })
    }

    /// Offers `reps` and serves requests on a thread until another
    /// application takes the clipboard.
    pub(crate) fn write(&self, reps: Vec<ClipRep>) -> Result<()> {
        let sources = encode(WAYLAND_TEXT, &reps)
            .into_iter()
            .map(|(mime, data)| MimeSource {
                source: Source::Bytes(data.into_boxed_slice()),
                mime_type: CopyMime::Specific(mime),
            })
            .collect();
        let before = self.changes.count();
        let mut options = Options::new();
        options.foreground(true);
        let prepared = options.prepare_copy_multi(sources).map_err(os)?;
        // Serving ends when another application takes the clipboard, or the
        // compositor goes away; neither is a failure.
        std::thread::spawn(move || {
            let _ = prepared.serve();
        });
        self.changes.wait_past(before);
        Ok(())
    }
}

impl Drop for DataControl {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.watcher.take().unwrap().join().unwrap();
    }
}
