//! The data object of a drag of files that do not exist on this PC yet.
//!
//! It offers `FileGroupDescriptorW` and one `FileContents` stream per name.
//! The contents are empty: like the macOS file promise, dropping creates the
//! named files (empty) and the transfer then writes the real bytes into the
//! folder the drop landed in. It takes part in the shell's asynchronous copy
//! (`IDataObjectAsyncCapability`): `DoDragDrop` returns about 35 ms after the
//! button is released, Explorer reads the contents afterwards on threads of
//! its own and ends with `EndOperation`, and the object must stay alive until
//! then (hardware probe 3).
//!
//! OLE and the drag-image helper call a data object from threads other than
//! the one that started the drag, and other apps may read `FileContents` as
//! soon as the pointer passes over them, so every field here is thread-safe
//! and nothing waits for a drop.

use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::{
    DV_E_FORMATETC, DV_E_LINDEX, E_NOTIMPL, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, S_OK,
};
use windows::Win32::System::Com::{
    DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IBindCtx, IDataObject, IEnumFORMATETC,
    IEnumSTATDATA, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock};
use windows::Win32::System::Ole::ReleaseStgMedium;
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, FD_PROGRESSUI, FILEDESCRIPTORW,
    IDataObjectAsyncCapability, SHCreateMemStream, SHCreateStdEnumFmtEtc,
};
use windows::core::{BOOL, HRESULT, Ref, Result, implement};

use windows::Win32::Foundation::DATA_S_SAMEFORMATETC;

/// File-name characters Windows refuses or that would leave the folder the
/// drop lands in (path separators, drive and stream colons, wildcards).
const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Room in `FILEDESCRIPTORW::cFileName`, with its terminator.
const NAME_UNITS: usize = 259;

/// `name` as the UTF-16 file name of a descriptor: forbidden characters
/// become `_`, and what does not fit is cut.
pub(crate) fn descriptor_name(name: &str) -> Vec<u16> {
    name.chars()
        .map(|c| if FORBIDDEN.contains(&c) { '_' } else { c })
        .collect::<String>()
        .encode_utf16()
        .take(NAME_UNITS)
        .collect()
}

/// A `FILEGROUPDESCRIPTORW` block for `names`: a count, then one descriptor
/// each.
pub(crate) fn descriptor_block(names: &[String]) -> Result<HGLOBAL> {
    let size = size_of::<u32>() + names.len() * size_of::<FILEDESCRIPTORW>();
    let block = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size) }?;
    let at = unsafe { GlobalLock(block) }.cast::<u8>();
    unsafe { at.cast::<u32>().write_unaligned(names.len() as u32) };
    let first = unsafe { at.add(size_of::<u32>()) }.cast::<FILEDESCRIPTORW>();
    for (i, name) in names.iter().enumerate() {
        let mut descriptor = FILEDESCRIPTORW {
            dwFlags: FD_PROGRESSUI.0 as u32,
            ..Default::default()
        };
        let mut file_name = [0u16; 260];
        for (slot, unit) in file_name.iter_mut().zip(descriptor_name(name)) {
            *slot = unit;
        }
        descriptor.cFileName = file_name;
        unsafe { first.add(i).write_unaligned(descriptor) };
    }
    let _ = unsafe { windows::Win32::System::Memory::GlobalUnlock(block) };
    Ok(block)
}

/// Where a shell copy ended; waited on by the thread that ran the drag.
#[derive(Default)]
pub(crate) struct Ended {
    started: AtomicBool,
    result: Mutex<Option<HRESULT>>,
    changed: Condvar,
}

impl Ended {
    /// Whether the shell has started its asynchronous copy.
    pub(crate) fn started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    fn set(&self, result: HRESULT) {
        *self.result.lock().unwrap() = Some(result);
        self.changed.notify_all();
    }

    /// The result `EndOperation` reported, waiting up to `limit` for it.
    pub(crate) fn wait(&self, limit: Duration) -> Option<HRESULT> {
        let guard = self.result.lock().unwrap();
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, limit, |r| r.is_none())
            .unwrap();
        *guard
    }
}

#[implement(IDataObject, IDataObjectAsyncCapability)]
pub(crate) struct VirtualFiles {
    names: Vec<String>,
    descriptor: u32,
    contents: u32,
    async_mode: AtomicBool,
    in_operation: AtomicBool,
    ended: Arc<Ended>,
}

impl VirtualFiles {
    pub(crate) fn new(names: Vec<String>, ended: Arc<Ended>) -> Self {
        let register = |name| unsafe { RegisterClipboardFormatW(name) };
        VirtualFiles {
            names,
            descriptor: register(CFSTR_FILEDESCRIPTORW),
            contents: register(CFSTR_FILECONTENTS),
            async_mode: AtomicBool::new(true),
            in_operation: AtomicBool::new(false),
            ended,
        }
    }

    fn format(&self, id: u32, medium: i32) -> FORMATETC {
        FORMATETC {
            cfFormat: id as u16,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: medium as u32,
        }
    }

    fn formats(&self) -> [FORMATETC; 2] {
        [
            self.format(self.descriptor, TYMED_HGLOBAL.0),
            self.format(self.contents, TYMED_ISTREAM.0),
        ]
    }
}

/// The wrapped object, for code that holds the interface and the state.
pub(crate) fn new_data_object(names: Vec<String>, ended: Arc<Ended>) -> IDataObject {
    VirtualFiles::new(names, ended).into()
}

impl windows::Win32::System::Com::IDataObject_Impl for VirtualFiles_Impl {
    fn GetData(&self, format: *const FORMATETC) -> Result<STGMEDIUM> {
        let format = unsafe { &*format };
        if u32::from(format.cfFormat) == self.descriptor
            && format.tymed & TYMED_HGLOBAL.0 as u32 != 0
        {
            return Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 {
                    hGlobal: descriptor_block(&self.names)?,
                },
                pUnkForRelease: ManuallyDrop::new(None),
            });
        }
        let known = u32::from(format.cfFormat) == self.contents
            && format.tymed & TYMED_ISTREAM.0 as u32 != 0;
        if !known {
            return Err(DV_E_FORMATETC.into());
        }
        if usize::try_from(format.lindex).map_or(true, |i| i >= self.names.len()) {
            return Err(DV_E_LINDEX.into());
        }
        Ok(STGMEDIUM {
            tymed: TYMED_ISTREAM.0 as u32,
            u: STGMEDIUM_0 {
                pstm: ManuallyDrop::new(unsafe { SHCreateMemStream(None) }),
            },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }

    fn GetDataHere(&self, _format: *const FORMATETC, _medium: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, format: *const FORMATETC) -> HRESULT {
        let format = unsafe { &*format };
        let offered = self
            .formats()
            .iter()
            .any(|f| f.cfFormat == format.cfFormat && f.tymed & format.tymed != 0);
        if offered { S_OK } else { DV_E_FORMATETC }
    }

    fn GetCanonicalFormatEtc(&self, _input: *const FORMATETC, out: *mut FORMATETC) -> HRESULT {
        unsafe { (*out).ptd = std::ptr::null_mut() };
        DATA_S_SAMEFORMATETC
    }

    /// The shell reports what it did (`TargetCLSID`, `Performed DropEffect`)
    /// through here; nothing in it changes what this object offers.
    fn SetData(
        &self,
        _format: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> Result<()> {
        if release.as_bool() {
            unsafe { ReleaseStgMedium(medium.cast_mut()) };
        }
        Ok(())
    }

    fn EnumFormatEtc(&self, direction: u32) -> Result<IEnumFORMATETC> {
        if direction == DATADIR_GET.0 as u32 {
            unsafe { SHCreateStdEnumFmtEtc(&self.formats()) }
        } else {
            Err(E_NOTIMPL.into())
        }
    }

    fn DAdvise(
        &self,
        _format: *const FORMATETC,
        _flags: u32,
        _sink: Ref<IAdviseSink>,
    ) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _connection: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

impl windows::Win32::UI::Shell::IDataObjectAsyncCapability_Impl for VirtualFiles_Impl {
    fn SetAsyncMode(&self, on: BOOL) -> Result<()> {
        self.async_mode.store(on.as_bool(), Ordering::SeqCst);
        Ok(())
    }

    fn GetAsyncMode(&self) -> Result<BOOL> {
        Ok(self.async_mode.load(Ordering::SeqCst).into())
    }

    fn StartOperation(&self, _context: Ref<IBindCtx>) -> Result<()> {
        self.ended.started.store(true, Ordering::SeqCst);
        self.in_operation.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn InOperation(&self) -> Result<BOOL> {
        Ok(self.in_operation.load(Ordering::SeqCst).into())
    }

    fn EndOperation(&self, result: HRESULT, _context: Ref<IBindCtx>, _effects: u32) -> Result<()> {
        self.in_operation.store(false, Ordering::SeqCst);
        self.ended.set(result);
        Ok(())
    }
}
