//! Files opened from Finder: double-click, Open With, or dropped on the Dock
//! icon.
//!
//! macOS delivers these as an "open documents" Apple Event. winit doesn't
//! pass it on, so we install our own handler. It has to be installed when
//! the app sends its "will finish launching" notification, because AppKit
//! installs its default handlers just before that and ours must replace them.
//! The paths are queued here and picked up by the app on its next frame.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::NSApplicationWillFinishLaunchingNotification;
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter,
    NSObjectProtocol,
};

/// Four-character Apple Event codes.
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

define_class!(
    // SAFETY: NSObject has no subclassing requirements and we don't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlothOpenDocumentsHandler"]
    struct OpenHandler;

    impl OpenHandler {
        #[unsafe(method(applicationWillFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            let manager = NSAppleEventManager::sharedAppleEventManager();
            let target: &AnyObject = self.as_ref();
            // SAFETY: `self` implements the selector with the signature
            // AppKit expects, and lives for the rest of the process.
            unsafe {
                manager.setEventHandler_andSelector_forEventClass_andEventID(
                    target,
                    sel!(handleOpenDocuments:withReplyEvent:),
                    CORE_EVENT_CLASS,
                    OPEN_DOCUMENTS,
                );
            }
        }

        #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
        fn handle_open_documents(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            let paths = paths_from_event(event);
            if paths.is_empty() {
                return;
            }
            PENDING.lock().unwrap().extend(paths);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }
    }

    unsafe impl NSObjectProtocol for OpenHandler {}
);

/// Call once from `main`, before the app starts running.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let handler: Retained<OpenHandler> = {
        let this = OpenHandler::alloc(mtm).set_ivars(());
        // SAFETY: plain NSObject initialiser.
        unsafe { msg_send![super(this), init] }
    };
    let observer: &AnyObject = handler.as_ref();
    // SAFETY: the selector matches a method on `handler`, and the observer
    // is kept alive below (the notification centre doesn't retain it).
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            observer,
            sel!(applicationWillFinishLaunching:),
            Some(NSApplicationWillFinishLaunchingNotification),
            None,
        );
    }
    // The handler must live for the whole process.
    std::mem::forget(handler);
}

/// Lets the handler wake the app when files arrive while it's running.
pub fn set_context(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

/// Files opened from Finder since the last call.
pub fn take_pending() -> Vec<PathBuf> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

fn paths_from_event(event: &NSAppleEventDescriptor) -> Vec<PathBuf> {
    let Some(list) = event.paramDescriptorForKeyword(DIRECT_OBJECT) else {
        return Vec::new();
    };
    // Usually a list of files; a single file may come on its own.
    let count = list.numberOfItems();
    let items: Vec<Retained<NSAppleEventDescriptor>> = if count > 0 {
        (1..=count).filter_map(|i| list.descriptorAtIndex(i)).collect()
    } else {
        vec![list]
    };
    items
        .iter()
        .filter_map(|d| d.fileURLValue())
        .filter_map(|url| url.path())
        .map(|p| PathBuf::from(p.to_string()))
        .collect()
}
