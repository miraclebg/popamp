//! Files Finder opens with Popamp (a double-click, "Open With", a drop on
//! the Dock icon), and a click on the Dock icon of a running Popamp. macOS
//! sends these as Apple Events, not on the command line, and winit does not
//! pass them on; this takes them and hands them over as the control socket
//! would.

use crate::cli::Remote;
use crate::ipc::Request;
use crate::model::AppEvent;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSNotificationCenter, NSString};
use winit::event_loop::EventLoopProxy;

/// `'aevt'`, `'odoc'` and `'----'`: the "open documents" event and its list
/// of files.
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
/// `'rapp'`: the Dock icon of the running app was clicked.
const REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

/// What the receiver needs: the way to the event loop.
struct Ivars {
    proxy: EventLoopProxy<AppEvent>,
}

define_class!(
    /// Receives the launch notification and the "open documents" events.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PopampFinderEvents"]
    #[ivars = Ivars]
    struct Receiver;

    impl Receiver {
        /// AppKit puts in its own event handlers while it finishes
        /// launching, replacing any put in earlier; this comes after
        /// those, and before the event that launched Popamp is handled.
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &AnyObject) {
            log::debug!("listening for files from Finder");
            let events = NSAppleEventManager::sharedAppleEventManager();
            // SAFETY: `self` has the selectors, with the arguments they take.
            unsafe {
                events.setEventHandler_andSelector_forEventClass_andEventID(
                    self,
                    sel!(openDocuments:withReplyEvent:),
                    CORE_EVENT_CLASS as _,
                    OPEN_DOCUMENTS as _,
                );
                events.setEventHandler_andSelector_forEventClass_andEventID(
                    self,
                    sel!(reopen:withReplyEvent:),
                    CORE_EVENT_CLASS as _,
                    REOPEN_APPLICATION as _,
                );
            }
        }

        /// The Dock icon was clicked: the windows come back, minimized or
        /// hidden ones too.
        #[unsafe(method(reopen:withReplyEvent:))]
        fn reopen(&self, _event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            let (tx, _rx) = crossbeam_channel::bounded(1);
            let _ = self.ivars().proxy.send_event(AppEvent::Remote(Request { remotes: vec![Remote::Show], reply: tx }));
        }

        #[unsafe(method(openDocuments:withReplyEvent:))]
        fn open_documents(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            let paths = event.paramDescriptorForKeyword(DIRECT_OBJECT as _).map(|list| file_paths(&list)).unwrap_or_default();
            log::debug!("Finder opens {paths:?}");
            if paths.is_empty() {
                return;
            }
            let (tx, _rx) = crossbeam_channel::bounded(1);
            let _ = self.ivars().proxy.send_event(AppEvent::Remote(Request { remotes: vec![Remote::Open(paths)], reply: tx }));
        }
    }
);

/// Starts listening. Called on the main thread once the event loop exists
/// and before it runs; the receiver lives as long as the program.
pub fn listen(proxy: EventLoopProxy<AppEvent>) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("not on the main thread: files from Finder will not open");
        return;
    };
    let receiver = Receiver::alloc(mtm).set_ivars(Ivars { proxy });
    // SAFETY: NSObject's `init` on a freshly allocated object.
    let receiver: Retained<Receiver> = unsafe { msg_send![super(receiver), init] };
    let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
    // SAFETY: `receiver` has the selector, taking the notification.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(&receiver, sel!(willFinishLaunching:), Some(&name), None)
    };
    // The notification centre does not keep its observers alive.
    std::mem::forget(receiver);
}

/// The paths of the files in an event's list; a single file counts too.
fn file_paths(list: &NSAppleEventDescriptor) -> Vec<String> {
    let path = |d: &NSAppleEventDescriptor| Some(d.fileURLValue()?.path()?.to_string());
    match list.numberOfItems() {
        0 => path(list).into_iter().collect(),
        n => (1..=n).filter_map(|i| list.descriptorAtIndex(i).and_then(|d| path(&d))).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{NSString, NSURL};

    #[test]
    fn the_dock_icon_and_finder_send_different_events() {
        assert_eq!(REOPEN_APPLICATION, u32::from_be_bytes(*b"rapp"));
        assert_ne!(REOPEN_APPLICATION, OPEN_DOCUMENTS);
    }

    #[test]
    fn an_open_event_gives_the_paths_of_its_files() {
        let list = NSAppleEventDescriptor::listDescriptor();
        for (i, p) in ["/tmp/a b.pls", "/tmp/Мечта.mp3"].iter().enumerate() {
            let url = NSURL::fileURLWithPath(&NSString::from_str(p));
            list.insertDescriptor_atIndex(&NSAppleEventDescriptor::descriptorWithFileURL(&url), (i + 1) as isize);
        }
        assert_eq!(file_paths(&list), ["/tmp/a b.pls", "/tmp/Мечта.mp3"]);
    }
}
