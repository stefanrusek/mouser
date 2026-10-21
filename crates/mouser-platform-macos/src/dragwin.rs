//! AppKit side of `FileDrag`: the drag pasteboard probe and the transparent
//! window that sources a drag of promised files.
//!
//! None of this can run in a unit test: starting a drag needs the main run
//! loop and a real mouse-down. It is exercised by the hardware conformance
//! test only.

use crate::drag::{DragProbe, DragStarter};
use block2::DynBlock;
use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dispatch2::DispatchQueue;
use mouser_platform::{Error, Result};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{
    AllocAnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class,
    msg_send,
};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSDragOperation, NSDraggingContext, NSDraggingItem,
    NSDraggingSession, NSDraggingSource, NSEvent, NSEventModifierFlags, NSEventType,
    NSFilePromiseProvider, NSFilePromiseProviderDelegate, NSPasteboard, NSPasteboardNameDrag,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    NSArray, NSError, NSObject, NSObjectProtocol, NSPoint, NSProcessInfo, NSRect, NSSize, NSString,
    NSURL,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::mpsc::{Sender, channel};

/// Side of the transparent window, in points.
const WINDOW_SIDE: f64 = 32.0;

/// Reads the real drag pasteboard and mouse state.
pub(crate) struct SystemProbe;

impl DragProbe for SystemProbe {
    fn button_down(&self) -> bool {
        NSEvent::pressedMouseButtons() & 1 != 0
    }

    fn files(&self) -> Vec<String> {
        let board = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag });
        let classes = NSArray::from_slice(&[NSURL::class()]);
        let objects = unsafe { board.readObjectsForClasses_options(&classes, None) };
        objects
            .into_iter()
            .flatten()
            .filter_map(|object| object.downcast::<NSURL>().ok())
            .filter(|url| url.isFileURL())
            .map(|url| url.path().expect("file URL has a path").to_string())
            .collect()
    }

    fn change_count(&self) -> u64 {
        NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag }).changeCount() as u64
    }
}

struct Ivars {
    /// Receives the first file the destination asked for.
    sent: Sender<PathBuf>,
    window: Retained<NSWindow>,
    /// Promises not yet written; the source keeps itself alive until none are.
    remaining: Cell<usize>,
    keepalive: RefCell<Option<Retained<DragSource>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MouserDragSource"]
    #[ivars = Ivars]
    struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended_at_point(
            &self,
            _session: &NSDraggingSession,
            _point: NSPoint,
            operation: NSDragOperation,
        ) {
            self.ivars().window.close();
            if operation == NSDragOperation::None {
                // Cancelled: nothing will be promised; dropping the sender
                // tells the waiting caller.
                self.ivars().keepalive.borrow_mut().take();
            }
        }
    }

    // The generated protocol binding of this method takes a stray main-thread
    // marker argument, so it is registered here by selector instead.
    impl DragSource {
        #[unsafe(method(filePromiseProvider:fileNameForType:))]
        fn file_name_for_type(
            &self,
            provider: &NSFilePromiseProvider,
            _file_type: &NSString,
        ) -> *mut NSString {
            let name = provider.userInfo().expect("promise carries its name");
            let name = name
                .downcast::<NSString>()
                .expect("promised name is a string");
            Retained::autorelease_return(name)
        }
    }

    unsafe impl NSFilePromiseProviderDelegate for DragSource {
        #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
        fn write_promise(
            &self,
            _provider: &NSFilePromiseProvider,
            url: &NSURL,
            completion: &DynBlock<dyn Fn(*mut NSError)>,
        ) {
            let path = PathBuf::from(url.path().expect("promise URL has a path").to_string());
            // The bytes arrive later over the network; the destination only
            // needs a file to exist so the drop completes.
            std::fs::write(&path, b"").expect("write promised placeholder");
            let _ = self.ivars().sent.send(path);
            completion.call((std::ptr::null_mut(),));
            let remaining = self.ivars().remaining.get() - 1;
            self.ivars().remaining.set(remaining);
            if remaining == 0 {
                self.ivars().keepalive.borrow_mut().take();
            }
        }
    }
);

/// On the main thread: the transparent window under the cursor and the drag.
fn start(mtm: MainThreadMarker, names: Vec<String>, at: (f64, f64), sent: Sender<PathBuf>) {
    let half = WINDOW_SIDE / 2.0;
    let frame = NSRect::new(
        NSPoint::new(at.0 - half, at.1 - half),
        NSSize::new(WINDOW_SIDE, WINDOW_SIDE),
    );
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.orderFrontRegardless();
    let view = window.contentView().expect("window has a content view");

    let source = mtm.alloc::<DragSource>().set_ivars(Ivars {
        sent,
        window: window.clone(),
        remaining: Cell::new(names.len()),
        keepalive: RefCell::new(None),
    });
    let source: Retained<DragSource> = unsafe { msg_send![super(source), init] };
    *source.ivars().keepalive.borrow_mut() = Some(source.clone());

    let delegate = ProtocolObject::from_ref(&*source);
    let items: Vec<Retained<NSDraggingItem>> = names
        .iter()
        .map(|name| {
            let provider = NSFilePromiseProvider::initWithFileType_delegate(
                NSFilePromiseProvider::alloc(),
                &NSString::from_str("public.data"),
                delegate,
            );
            let name = NSString::from_str(name);
            unsafe { provider.setUserInfo(Some(&*name as &AnyObject)) };
            let item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                ProtocolObject::from_ref(&*provider),
            );
            unsafe {
                item.setDraggingFrame_contents(
                    NSRect::new(
                        NSPoint::new(0.0, 0.0),
                        NSSize::new(WINDOW_SIDE, WINDOW_SIDE),
                    ),
                    None,
                )
            };
            item
        })
        .collect();
    let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseDown,
        NSPoint::new(half, half),
        NSEventModifierFlags::empty(),
        NSProcessInfo::processInfo().systemUptime(),
        window.windowNumber(),
        None,
        0,
        1,
        1.0,
    )
    .expect("synthesised mouse-down");
    view.beginDraggingSessionWithItems_event_source(
        &NSArray::from_retained_slice(&items),
        &event,
        ProtocolObject::from_ref(&*source),
    );
}

/// Starts real drags from the main thread.
pub(crate) struct SystemStarter;

impl DragStarter for SystemStarter {
    fn pointer(&self) -> ((f64, f64), f64) {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap();
        let at: CGPoint = CGEvent::new(source).unwrap().location();
        ((at.x, at.y), CGDisplay::main().bounds().size.height)
    }

    /// Must not be called on the main thread, which it hands the drag to and
    /// which must stay free to run it.
    fn run(&self, names: Vec<String>, at: (f64, f64)) -> Result<PathBuf> {
        let (sent, dropped) = channel();
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().expect("main queue runs on the main thread");
            start(mtm, names, at, sent);
        });
        dropped
            .recv()
            .map_err(|_| Error::Os("drag cancelled".into()))
    }
}
