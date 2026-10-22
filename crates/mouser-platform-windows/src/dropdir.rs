//! Which folder a drag was dropped into.
//!
//! Explorer does not tell the source where it dropped (hardware probe 3), so
//! the folder is found from the drop point: the root window under it, then
//! the `IShellWindows` entry whose `IWebBrowserApp::HWND` is that window
//! (`IShellBrowser::GetWindow` returns an inner window and cannot be used to
//! match), and from it the active view's folder. The desktop is not a
//! browser window; its two window classes map to the Desktop folder.

use crate::inject::os;
use mouser_platform::{Error, Result};
use std::path::PathBuf;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IServiceProvider};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, IFolderView, IPersistFolder2, IShellBrowser, IShellWindows, IWebBrowserApp,
    KF_FLAG_DEFAULT, SHGetKnownFolderPath, SHGetPathFromIDListW, SID_STopLevelBrowser,
    ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetClassNameW, WindowFromPoint,
};
use windows::core::Interface;

/// The window classes of the desktop and its wallpaper host.
const DESKTOP_CLASSES: [&str; 2] = ["Progman", "WorkerW"];

pub(crate) fn is_desktop_class(class: &str) -> bool {
    DESKTOP_CLASSES.contains(&class)
}

fn class_of(hwnd: HWND) -> String {
    let mut name = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut name) }.max(0) as usize;
    String::from_utf16_lossy(&name[..len])
}

pub(crate) fn desktop_folder() -> Result<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None) };
    let path = os("SHGetKnownFolderPath(Desktop)", path)?;
    let folder = PathBuf::from(unsafe { path.to_string() }.unwrap_or_default());
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    Ok(folder)
}

/// The file-system folder the Explorer window `root` shows.
pub(crate) fn folder_of_explorer(root: HWND) -> Option<PathBuf> {
    let windows: IShellWindows =
        unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }.ok()?;
    let count = unsafe { windows.Count() }.ok()?;
    (0..count).find_map(|i| {
        let item = unsafe { windows.Item(&VARIANT::from(i)) }.ok()?;
        let shown = unsafe { item.cast::<IWebBrowserApp>().ok()?.HWND() }.ok()?;
        (shown.0 == root.0 as isize).then_some(())?;
        let browser: IShellBrowser = unsafe {
            item.cast::<IServiceProvider>()
                .ok()?
                .QueryService(&SID_STopLevelBrowser)
        }
        .ok()?;
        let view = unsafe { browser.QueryActiveShellView() }.ok()?;
        let folder: IPersistFolder2 =
            unsafe { view.cast::<IFolderView>().ok()?.GetFolder() }.ok()?;
        let pidl = unsafe { folder.GetCurFolder() }.ok()?;
        let mut path = [0u16; 260];
        let found = unsafe { SHGetPathFromIDListW(pidl, &mut path) }.as_bool();
        unsafe { CoTaskMemFree(Some(pidl.cast())) };
        let len = path.iter().position(|&c| c == 0).unwrap_or(path.len());
        found.then(|| PathBuf::from(String::from_utf16_lossy(&path[..len])))
    })
}

/// The folder a drop at physical pixel `at` landed in: an Explorer window's
/// folder, or the Desktop. Needs COM on the calling thread.
pub(crate) fn folder_at(at: POINT) -> Result<PathBuf> {
    let root = unsafe { GetAncestor(WindowFromPoint(at), GA_ROOT) };
    folder_for(&class_of(root), root)
}

/// The folder for a root window of class `class`.
pub(crate) fn folder_for(class: &str, root: HWND) -> Result<PathBuf> {
    if is_desktop_class(class) {
        return desktop_folder();
    }
    folder_of_explorer(root)
        .ok_or_else(|| Error::Os("the files were dropped on something that is not a folder".into()))
}
