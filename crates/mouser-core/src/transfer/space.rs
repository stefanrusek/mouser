//! The free-space check (Story 7 scenario 4): the whole transfer must fit
//! before any byte is written.

use std::path::Path;
use std::time::Instant;

use mouser_proto::wire::{ControlMessage, TransferId};

use super::engine::{Out, State, Transfers};
use super::progress;

/// The `TransferReject` reason when the destination is too small.
pub const NO_SPACE: &str = "no_space";

/// What the user is told.
pub const NO_SPACE_TEXT: &str =
    "There is not enough free space on that computer for these files, so nothing was copied.";

/// Whether `total` bytes fit in `free`.
pub fn fits(total: u64, free: u64) -> bool {
    total <= free
}

/// Bytes available to this user in the volume holding `dir`.
#[cfg(unix)]
pub fn free_space(dir: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).expect("path without NUL");
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a NUL-terminated string and `stats` is writable
    // memory of the size statvfs expects.
    let status = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    assert_eq!(status, 0, "the drop folder's volume can be inspected");
    // SAFETY: statvfs succeeded, so it filled `stats` in.
    let stats = unsafe { stats.assume_init() };
    // The field types differ between operating systems; widening is lossless on all.
    let bytes = u128::from(stats.f_bavail) * u128::from(stats.f_frsize);
    u64::try_from(bytes).expect("a volume has fewer than 2^64 bytes")
}

/// Bytes available to this user in the volume holding `dir`.
#[cfg(windows)]
pub fn free_space(dir: &Path) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated and `free` is a valid out pointer;
    // the other two outputs are optional.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "the drop folder's volume can be inspected");
    free
}

impl Transfers {
    /// The destination: the dropped files do not fit. Nothing was written;
    /// the source is told and so is the user.
    pub(super) fn reject_for_space(&mut self, id: TransferId, now: Instant) -> Vec<Out> {
        let transfer = self
            .inbound
            .get_mut(&id)
            .expect("a rejected transfer exists");
        transfer.state = State::Rejected;
        let from = transfer.from;
        self.ids.remove(&id);
        let mut note =
            progress::notification(&id, State::Rejected, &transfer.progress.snapshot(now));
        note["reason"] = NO_SPACE.into();
        note["message"] = NO_SPACE_TEXT.into();
        vec![
            Out::To(
                from,
                ControlMessage::TransferReject {
                    transfer_id: id,
                    reason: NO_SPACE.into(),
                },
            ),
            Out::Notify(note),
        ]
    }
}
