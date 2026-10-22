//! File drag between computers (Story 7): not possible on Linux. X11 and
//! Wayland have no way to start or observe a drag across the pointer
//! hand-off (InputCapture even cancels a drag on GNOME), so the backend
//! offers no `FileDrag` and states the limitation (plan D3 item 3).

pub(crate) const NO_FILE_DRAG: &str =
    "Dragging files between computers isn't available on Linux; use copy and paste";
