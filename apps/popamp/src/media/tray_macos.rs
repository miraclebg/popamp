//! The tray icon on macOS: a note in the menu bar with the tray's menu,
//! while the user wants it. AppKit wants the main thread, so the tray's
//! thread hands each change to the main queue, where the item lives.

use super::{now_playing, tray_menu, Entry, Sender, Snapshot};
use crate::cli::Remote;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSControlStateValueOff, NSControlStateValueOn, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::NSString;
use std::cell::RefCell;

/// The icon's tooltip: the name, then what plays.
fn tooltip(snap: &Snapshot) -> String {
    format!("Popamp\n{}", now_playing(snap))
}

/// Runs `work` on the main thread, where AppKit is.
fn on_main(work: impl FnOnce(MainThreadMarker) + Send + 'static) {
    DispatchQueue::main().exec_async(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            work(mtm);
        }
    });
}

/// What the menu items send, by their tags.
struct Ivars {
    send: Sender,
    remotes: RefCell<Vec<Remote>>,
}

define_class!(
    /// Receives the menu's choices.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PopampTrayMenu"]
    #[ivars = Ivars]
    struct Receiver;

    impl Receiver {
        #[unsafe(method(chosen:))]
        fn chosen(&self, item: &NSMenuItem) {
            let remote = self.ivars().remotes.borrow().get(item.tag() as usize).cloned();
            if let Some(remote) = remote {
                (self.ivars().send)(vec![remote]);
            }
        }
    }
);

/// The item in the menu bar, on the main thread.
struct Item {
    status: Retained<NSStatusItem>,
    receiver: Retained<Receiver>,
    menu: Vec<Option<Entry>>,
}

thread_local! {
    static ITEM: RefCell<Option<Item>> = const { RefCell::new(None) };
}

/// The icon while shown; dropping it takes the icon down.
struct Shown;

impl Shown {
    fn new(send: Sender) -> Option<Shown> {
        on_main(move |mtm| {
            let status = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            if let Some(button) = status.button(mtm) {
                let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str("music.note"),
                    Some(&NSString::from_str("Popamp")),
                );
                match image {
                    Some(image) => {
                        // A template image follows the menu bar's light or dark look.
                        image.setTemplate(true);
                        button.setImage(Some(&image));
                    }
                    None => button.setTitle(&NSString::from_str("♫")),
                }
            }
            let receiver = Receiver::alloc(mtm).set_ivars(Ivars { send, remotes: RefCell::new(Vec::new()) });
            // SAFETY: NSObject's `init` on a freshly allocated object.
            let receiver: Retained<Receiver> = unsafe { msg_send![super(receiver), init] };
            ITEM.with(|i| *i.borrow_mut() = Some(Item { status, receiver, menu: Vec::new() }));
        });
        Some(Shown)
    }

    /// Shows `snap` in the tooltip and rebuilds the menu if it changed.
    fn refresh(&self, snap: &Snapshot) {
        let (tip, menu) = (tooltip(snap), tray_menu(snap));
        on_main(move |mtm| {
            ITEM.with(|i| {
                let mut item = i.borrow_mut();
                let Some(item) = item.as_mut() else { return };
                if let Some(button) = item.status.button(mtm) {
                    button.setToolTip(Some(&NSString::from_str(&tip)));
                }
                if item.menu != menu {
                    item.status.setMenu(Some(&build(mtm, &item.receiver, &menu)));
                    item.menu = menu;
                }
            });
        });
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        on_main(|_| {
            if let Some(item) = ITEM.with(|i| i.borrow_mut().take()) {
                NSStatusBar::systemStatusBar().removeStatusItem(&item.status);
            }
        });
    }
}

/// The menu of `entries`, each item sending its remote through `receiver`.
fn build(mtm: MainThreadMarker, receiver: &Receiver, entries: &[Option<Entry>]) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let mut remotes = Vec::new();
    for entry in entries {
        let Some(entry) = entry else {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            continue;
        };
        // SAFETY: a menu item with an action `receiver` implements.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(entry.label),
                Some(sel!(chosen:)),
                &NSString::from_str(""),
            )
        };
        // SAFETY: the receiver outlives the menu (both live in `Item`).
        unsafe { item.setTarget(Some(receiver as &AnyObject)) };
        item.setTag(remotes.len() as isize);
        if let Some(on) = entry.checked {
            item.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
        }
        menu.addItem(&item);
        remotes.push(entry.remote.clone());
    }
    *receiver.ivars().remotes.borrow_mut() = remotes;
    menu
}

/// The menu-bar icon while the user wants one.
pub struct Tray {
    send: Sender,
    shown: Option<Option<Shown>>,
}

impl Tray {
    pub fn new(send: Sender) -> Tray {
        Tray { send, shown: None }
    }

    /// Shows, updates or takes down the icon to match `snap`.
    pub fn update(&mut self, snap: &Snapshot) {
        let send = &self.send;
        crate::hold::update(&mut self.shown, snap.tray, || Shown::new(send.clone()));
        if let Some(Some(shown)) = &self.shown {
            shown.refresh(snap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_bar_shows_what_plays() {
        assert_eq!(tooltip(&Snapshot::default()), "Popamp\nStopped");
    }
}
