//! `Clipboard` on the general `NSPasteboard`.
//!
//! The mapping between `ClipRep`s and pasteboard types, and the path
//! normalisation, are pure and run against the `Board` seam; `SystemBoard` is
//! the clipboard-rs / `NSPasteboard` implementation.

use clipboard_rs::{Clipboard as _, ClipboardContent, ClipboardContext};
use mouser_platform::{ClipRep, Clipboard, Error, Result};
use objc2_app_kit::NSPasteboard;
use std::path::PathBuf;

pub(crate) const TEXT: &str = "public.utf8-plain-text";
pub(crate) const HTML: &str = "public.html";
pub(crate) const RTF: &str = "public.rtf";
pub(crate) const PNG: &str = "public.png";

/// One thing to put on the pasteboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    Buffer(&'static str, Vec<u8>),
    Files(Vec<String>),
}

/// The pasteboard itself; a seam so the mapping is testable.
pub(crate) trait Board {
    /// The bytes held under a pasteboard type, if any.
    fn buffer(&self, format: &str) -> Option<Vec<u8>>;
    /// File paths held on the pasteboard, empty if none.
    fn files(&self) -> Vec<String>;
    /// Replace the whole pasteboard with `items`, as one write.
    fn write(&self, items: Vec<Item>) -> Result<()>;
    fn change_count(&self) -> u64;
}

/// Percent-decodes `%XX` escapes; anything malformed is kept verbatim.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A path as every computer will see it: `file://` URLs become paths, and the
/// `/private` prefix of the firmlinked `/var`, `/tmp` and `/etc` is dropped
/// (the pasteboard reports `/var/...` where `realpath` says `/private/var/...`).
pub(crate) fn normalise_path(raw: &str) -> PathBuf {
    let path = match raw.strip_prefix("file://") {
        Some(url) => percent_decode(url.strip_prefix("localhost").unwrap_or(url)),
        None => raw.to_string(),
    };
    for dir in ["var", "tmp", "etc"] {
        if let Some(rest) = path.strip_prefix(&format!("/private/{dir}"))
            && (rest.is_empty() || rest.starts_with('/'))
        {
            return PathBuf::from(format!("/{dir}{rest}"));
        }
    }
    PathBuf::from(path)
}

fn text_of(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("pasteboard text is UTF-8")
}

/// Every representation present on the pasteboard.
pub(crate) fn read(board: &dyn Board) -> Vec<ClipRep> {
    let mut reps = Vec::new();
    if let Some(bytes) = board.buffer(TEXT) {
        reps.push(ClipRep::Text(text_of(bytes)));
    }
    if let Some(bytes) = board.buffer(HTML) {
        reps.push(ClipRep::Html(text_of(bytes)));
    }
    if let Some(bytes) = board.buffer(RTF) {
        reps.push(ClipRep::Rtf(bytes));
    }
    if let Some(bytes) = board.buffer(PNG) {
        reps.push(ClipRep::Png(bytes));
    }
    let files: Vec<PathBuf> = board.files().iter().map(|f| normalise_path(f)).collect();
    if !files.is_empty() {
        reps.push(ClipRep::Files(files));
    }
    reps
}

/// Puts all of `reps` on the pasteboard in one write.
pub(crate) fn write(board: &dyn Board, reps: Vec<ClipRep>) -> Result<()> {
    let items = reps
        .into_iter()
        .map(|rep| match rep {
            ClipRep::Text(text) => Item::Buffer(TEXT, text.into_bytes()),
            ClipRep::Html(html) => Item::Buffer(HTML, html.into_bytes()),
            ClipRep::Rtf(bytes) => Item::Buffer(RTF, bytes),
            ClipRep::Png(bytes) => Item::Buffer(PNG, bytes),
            ClipRep::Files(paths) => Item::Files(
                paths
                    .iter()
                    .map(|p| p.to_str().expect("UTF-8 path").to_string())
                    .collect(),
            ),
        })
        .collect();
    board.write(items)
}

/// The general pasteboard through clipboard-rs.
pub(crate) struct SystemBoard {
    context: ClipboardContext,
}

impl SystemBoard {
    pub(crate) fn new() -> Result<Self> {
        let context = ClipboardContext::new().map_err(|e| Error::Os(e.to_string()))?;
        Ok(SystemBoard { context })
    }
}

impl Board for SystemBoard {
    fn buffer(&self, format: &str) -> Option<Vec<u8>> {
        self.context.get_buffer(format).ok()
    }

    fn files(&self) -> Vec<String> {
        self.context.get_files().unwrap_or_default()
    }

    fn write(&self, items: Vec<Item>) -> Result<()> {
        let contents = items
            .into_iter()
            .map(|item| match item {
                Item::Buffer(format, bytes) => ClipboardContent::Other(format.to_string(), bytes),
                Item::Files(files) => ClipboardContent::Files(files),
            })
            .collect();
        self.context
            .set(contents)
            .map_err(|e| Error::Os(e.to_string()))
    }

    fn change_count(&self) -> u64 {
        NSPasteboard::generalPasteboard().changeCount() as u64
    }
}

/// The macOS clipboard.
pub(crate) struct MacClipboard<B: Board = SystemBoard> {
    board: B,
}

impl<B: Board> MacClipboard<B> {
    pub(crate) fn new(board: B) -> Self {
        MacClipboard { board }
    }
}

impl<B: Board + Send> Clipboard for MacClipboard<B> {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        Ok(read(&self.board))
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        write(&self.board, reps)
    }

    fn change_count(&mut self) -> u64 {
        self.board.change_count()
    }
}
