//! `FileDrag`: report files being dragged out of this Mac, and start a drag
//! of promised files into this Mac.
//!
//! The decisions are pure and run against the `DragProbe` and `DragStarter`
//! seams. The AppKit side (`dragwin`) is the real implementation.

use mouser_platform::{DropTarget, Error, FileDrag, Result};
use std::path::{Path, PathBuf};

/// What the OS says about a drag in progress; a seam so the decision is
/// testable.
pub(crate) trait DragProbe {
    /// Whether the primary mouse button is held down.
    fn button_down(&self) -> bool;
    /// File paths (or `file://` URLs) on the drag pasteboard.
    fn files(&self) -> Vec<String>;
    /// The drag pasteboard's change counter. The drag pasteboard keeps its
    /// contents after a drag ends, so a change is what marks a new drag.
    fn change_count(&self) -> u64;
}

/// Starts a drag of promised files under the cursor.
pub(crate) trait DragStarter {
    /// The pointer position in global top-left coordinates, and the height of
    /// the primary display (for the bottom-left flip AppKit wants).
    fn pointer(&self) -> ((f64, f64), f64);
    /// Starts the drag at `at` (AppKit bottom-left coordinates) and blocks
    /// until a drop; returns the file the destination asked for.
    fn run(&self, names: Vec<String>, at: (f64, f64)) -> Result<PathBuf>;
}

/// Global top-left coordinates to AppKit's bottom-left origin.
pub(crate) fn appkit_point(point: (f64, f64), primary_height: f64) -> (f64, f64) {
    (point.0, primary_height - point.1)
}

/// The folder a drop landed in, given the first promised file's location.
pub(crate) fn destination(file: &Path) -> PathBuf {
    file.parent()
        .expect("promised file has a folder")
        .to_path_buf()
}

pub(crate) struct MacFileDrag<P: DragProbe, S: DragStarter> {
    probe: P,
    starter: S,
    /// The drag pasteboard's counter when last seen with the button up.
    idle_count: Option<u64>,
}

impl<P: DragProbe, S: DragStarter> MacFileDrag<P, S> {
    pub(crate) fn new(probe: P, starter: S) -> Self {
        MacFileDrag {
            probe,
            starter,
            idle_count: None,
        }
    }
}

impl<P: DragProbe + Send, S: DragStarter + Send> FileDrag for MacFileDrag<P, S> {
    fn outgoing(&mut self) -> Option<Vec<PathBuf>> {
        let count = self.probe.change_count();
        if !self.probe.button_down() {
            self.idle_count = Some(count);
            return None;
        }
        if self.idle_count == Some(count) {
            return None;
        }
        let files: Vec<PathBuf> = self
            .probe
            .files()
            .iter()
            .map(|f| crate::clipboard::normalise_path(f))
            .collect();
        (!files.is_empty()).then_some(files)
    }

    fn begin_incoming(&mut self, names: Vec<String>) -> Result<DropTarget> {
        if names.is_empty() {
            return Err(Error::Os("no files to drag".into()));
        }
        let (point, primary_height) = self.starter.pointer();
        let at = appkit_point(point, primary_height);
        let file = self.starter.run(names, at)?;
        Ok(DropTarget(destination(&file)))
    }
}
