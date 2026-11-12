//! AppKit, through winit. For now only what winit does by itself: skins
//! are drawn as rectangles, docked windows are not tied to the main one,
//! and the bar reserves no strip (macOS lets no program do that).

use super::{Mask, WindowRole};
use crate::bar::{Screen, Strip};
use std::sync::Once;
use winit::window::{Window, WindowAttributes, WindowLevel};

/// Fixed-size window attributes for `role`, borderless but for dialogs.
pub fn attributes(role: WindowRole) -> WindowAttributes {
    let attrs = Window::default_attributes().with_decorations(false).with_resizable(false);
    match role {
        WindowRole::Main | WindowRole::Docked | WindowRole::Bar => attrs,
        // Above the other windows, without taking the focus from the one
        // they opened from.
        WindowRole::Popup | WindowRole::Tooltip => attrs.with_window_level(WindowLevel::AlwaysOnTop).with_active(false),
        WindowRole::Dialog => attrs.with_decorations(true),
    }
}

/// Popamp moves and minimizes the docked windows itself, as on Linux.
pub const OWNED_FOLLOW_OWNER: bool = false;

/// Nothing to connect to yet.
pub struct Platform;

impl Platform {
    pub fn connect() -> Platform {
        Platform
    }

    /// Skin shapes are not drawn on macOS yet: the window stays whole.
    pub fn set_shape(&self, _window: &Window, mask: Option<&Mask>, _size: (i32, i32)) -> Result<(), String> {
        static SAID: Once = Once::new();
        if mask.is_some() {
            SAID.call_once(|| log::info!("skin shapes are not drawn on macOS yet"));
        }
        Ok(())
    }

    /// Nothing to reserve: macOS keeps no strip for other programs' bars.
    pub fn reserve(&self, _window: &Window, _strip: Strip) -> Result<(), String> {
        Ok(())
    }

    /// Not tied yet: each window is minimized and restored by itself.
    pub fn attach(&self, _window: &Window, _owner: Option<&Window>) -> Result<(), String> {
        Ok(())
    }

    /// Nothing more to do: menus and tooltips open without the focus.
    pub fn no_activate(&self, _window: &Window) {}

    /// Nothing to give back.
    pub fn release(&self, _window: &Window) {}

    /// Brings `windows` above other programs' windows, back to front,
    /// without making any of them the key window: macOS brings up only the
    /// window clicked, not its companions.
    pub fn raise_together(&self, windows: &[&Window]) {
        use objc2::runtime::AnyObject;
        use objc2::{msg_send, rc::Retained};
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        for window in windows {
            let Ok(handle) = window.window_handle() else { continue };
            let RawWindowHandle::AppKit(h) = handle.as_raw() else { continue };
            // SAFETY: winit's view, alive with its window, on the main thread.
            let view: &AnyObject = unsafe { h.ns_view.cast().as_ref() };
            let ns_window: Option<Retained<AnyObject>> = unsafe { msg_send![view, window] };
            if let Some(ns_window) = ns_window {
                // SAFETY: `orderFront:` takes any sender, nil included.
                let _: () = unsafe { msg_send![&*ns_window, orderFront: std::ptr::null::<AnyObject>()] };
            }
        }
    }

    /// None known yet; the bar then spans its whole monitor.
    pub fn work_areas(&self) -> Vec<Screen> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dialogs_have_a_title_bar() {
        for role in [WindowRole::Main, WindowRole::Docked, WindowRole::Bar, WindowRole::Popup, WindowRole::Tooltip] {
            assert!(!attributes(role).decorations, "{role:?}");
        }
        assert!(attributes(WindowRole::Dialog).decorations);
    }

    #[test]
    fn menus_and_tooltips_float_without_taking_the_focus() {
        for role in [WindowRole::Popup, WindowRole::Tooltip] {
            let a = attributes(role);
            assert!(!a.active && a.window_level == WindowLevel::AlwaysOnTop, "{role:?}");
        }
    }
}
