//! AppKit, through winit, and Core Animation for skin shapes: macOS cuts
//! no window to a shape, so a shaped skin's window is see-through and its
//! layer masked; and since clicks do not pass through a masked layer, the
//! window stops taking the mouse while the cursor is off its shape. The bar
//! reserves no strip (macOS lets no program do that).

use super::{shape_rects, Mask, WindowRole};
use crate::bar::{Screen, Strip};
use std::cell::{Cell, RefCell};
use winit::window::{Window, WindowAttributes, WindowId, WindowLevel};

/// Fixed-size window attributes for `role`, borderless but for dialogs.
pub fn attributes(role: WindowRole) -> WindowAttributes {
    let attrs = Window::default_attributes().with_decorations(false).with_resizable(false);
    match role {
        // See-through, so a skin's shape can show the desktop around it.
        WindowRole::Main | WindowRole::Docked | WindowRole::Bar => attrs.with_transparent(true),
        // Above the other windows, without taking the focus from the one
        // they opened from.
        WindowRole::Popup | WindowRole::Tooltip => attrs.with_window_level(WindowLevel::AlwaysOnTop).with_active(false),
        WindowRole::Dialog => attrs.with_decorations(true),
    }
}

/// Popamp moves and minimizes the docked windows itself, as on Linux.
pub const OWNED_FOLLOW_OWNER: bool = false;

/// macOS gives each minimized window a Dock tile of its own, so only the
/// main window goes there; its companions hide and come back with it.
pub const MINIMIZE_HIDES_COMPANIONS: bool = true;

/// A shaped window: its mask, and whether it takes the mouse now.
struct Shaped {
    id: WindowId,
    mask: Mask,
    takes_mouse: Cell<bool>,
}

/// The shaped windows, for passing clicks through what they cut away.
pub struct Platform {
    shapes: RefCell<Vec<Shaped>>,
}

impl Platform {
    pub fn connect() -> Platform {
        Platform { shapes: RefCell::new(Vec::new()) }
    }

    /// Shows only the pixels `mask` covers of a window `size` desktop units
    /// big, through a mask layer; all of it without a mask.
    pub fn set_shape(&self, window: &Window, mask: Option<&Mask>, size: (i32, i32)) -> Result<(), String> {
        layer::set_mask(window, mask.map(|_| shape_rects(mask, size)), size)?;
        let mut shapes = self.shapes.borrow_mut();
        shapes.retain(|s| s.id != window.id());
        match mask {
            Some(m) => shapes.push(Shaped { id: window.id(), mask: m.clone(), takes_mouse: Cell::new(true) }),
            None => {
                let _ = window.set_cursor_hittest(true);
            }
        }
        Ok(())
    }

    /// Whether any window is shaped, so the cursor is to be followed.
    pub fn tracks_cursor(&self) -> bool {
        !self.shapes.borrow().is_empty()
    }

    /// Lets each shaped window among `windows` take the mouse only while
    /// the cursor is over its shape, so clicks beside it reach what is
    /// behind. Never while a button is held: a drag keeps its window.
    pub fn track_cursor(&self, windows: &[&Window], button_down: bool) {
        let shapes = self.shapes.borrow();
        if shapes.is_empty() {
            return;
        }
        let Some(cursor) = layer::cursor() else { return };
        for window in windows {
            let Some(shape) = shapes.iter().find(|s| s.id == window.id()) else { continue };
            let Ok(at) = window.inner_position() else { continue };
            let origin = super::from_screen(at.x as f64, at.y as f64, window.scale_factor());
            let take = button_down || shape.mask.covers(cursor.0 - origin.0, cursor.1 - origin.1);
            if shape.takes_mouse.get() != take && window.set_cursor_hittest(take).is_ok() {
                shape.takes_mouse.set(take);
            }
        }
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

    /// What is on the clipboard: the paths of copied files, and the text.
    pub fn clipboard(&self) -> (Vec<String>, Option<String>) {
        use objc2::ClassType;
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::{NSArray, NSURL};
        let board = NSPasteboard::generalPasteboard();
        let classes = NSArray::from_slice(&[NSURL::class()]);
        // SAFETY: asks for NSURL objects, with no options.
        let urls = unsafe { board.readObjectsForClasses_options(&classes, None) };
        let files = urls
            .map(|list| {
                list.to_vec()
                    .into_iter()
                    .filter_map(|o| o.downcast::<NSURL>().ok())
                    .filter(|u| u.isFileURL())
                    .filter_map(|u| u.path())
                    .map(|p| p.to_string())
                    .collect()
            })
            .unwrap_or_default();
        // SAFETY: AppKit's constant for plain text.
        let text = board.stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string());
        (files, text)
    }

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

/// Core Animation and Core Graphics: the mask layer, and the cursor.
mod layer {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_core_graphics::{
        CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGEvent, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
        CGImageComponentInfo, CGImagePixelFormatInfo,
    };
    use objc2_quartz_core::{CALayer, CATransaction};
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    /// Frees the mask's pixels when Core Graphics is done with them.
    unsafe extern "C-unwind" fn release(_info: *mut c_void, data: NonNull<c_void>, size: usize) {
        let pixels = std::ptr::slice_from_raw_parts_mut(data.cast::<u32>().as_ptr(), size / 4);
        // SAFETY: the box handed to `CGDataProvider::with_data` below.
        drop(unsafe { Box::from_raw(pixels) })
    }

    /// Masks `window`'s layer to `rects` (in desktop units of a window
    /// `size` big), or removes the mask for `None`.
    pub fn set_mask(window: &Window, rects: Option<Vec<(i16, i16, u16, u16)>>, size: (i32, i32)) -> Result<(), String> {
        let Ok(handle) = window.window_handle() else { return Ok(()) };
        let RawWindowHandle::AppKit(h) = handle.as_raw() else { return Ok(()) };
        // SAFETY: winit's view, alive with its window, on the main thread.
        let view: &AnyObject = unsafe { h.ns_view.cast().as_ref() };
        let layer: Option<Retained<CALayer>> = unsafe { msg_send![view, layer] };
        let Some(layer) = layer else { return Err("the window has no layer".into()) };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let done = match rects {
            // SAFETY: removing a layer's mask.
            None => unsafe {
                layer.setMask(None);
                Ok(())
            },
            Some(rects) => mask_layer(&layer, rects, size),
        };
        CATransaction::commit();
        // The window's shadow follows its pixels; ask for it again.
        let ns_window: Option<Retained<AnyObject>> = unsafe { msg_send![view, window] };
        if let Some(ns_window) = ns_window {
            // SAFETY: a plain NSWindow call on the main thread.
            let _: () = unsafe { msg_send![&*ns_window, invalidateShadow] };
        }
        done
    }

    /// Sets `layer`'s mask to an image opaque where `rects` are.
    fn mask_layer(layer: &CALayer, rects: Vec<(i16, i16, u16, u16)>, size: (i32, i32)) -> Result<(), String> {
        let (w, h) = (size.0.max(1) as usize, size.1.max(1) as usize);
        let mut pixels = vec![0u32; w * h].into_boxed_slice();
        for (x, y, rw, rh) in rects {
            let (x, y) = (x.max(0) as usize, y.max(0) as usize);
            for row in y..(y + rh as usize).min(h) {
                pixels[row * w + x.min(w)..row * w + (x + rw as usize).min(w)].fill(0xff00_0000);
            }
        }
        let len = pixels.len() * 4;
        let data = Box::into_raw(pixels).cast::<c_void>();
        // SAFETY: `data` is `len` bytes, freed by `release`.
        let provider =
            unsafe { CGDataProvider::with_data(std::ptr::null_mut(), data, len, Some(release)) }.ok_or("no data provider for the mask")?;
        let info = CGBitmapInfo(
            CGImageAlphaInfo::PremultipliedFirst.0
                | CGImageComponentInfo::Integer.0
                | CGImageByteOrderInfo::Order32Little.0
                | CGImagePixelFormatInfo::Packed.0,
        );
        let space = CGColorSpace::new_device_rgb().ok_or("no colour space for the mask")?;
        // SAFETY: a `w` x `h` 32-bit image over the provider's bytes.
        let image = unsafe {
            CGImage::new(
                w,
                h,
                8,
                32,
                w * 4,
                Some(&space),
                info,
                Some(&provider),
                std::ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .ok_or("no mask image")?;
        let mask = CALayer::new();
        mask.setFrame(layer.bounds());
        // SAFETY: a CGImage is a valid layer content; a CALayer a valid mask.
        unsafe {
            mask.setContents(Some(image.as_ref()));
            layer.setMask(Some(&mask));
        }
        Ok(())
    }

    /// The cursor anywhere on the desktop, in points from the top-left of
    /// the main screen, as winit counts window positions.
    pub fn cursor() -> Option<(i32, i32)> {
        let event = CGEvent::new(None)?;
        let at = CGEvent::location(Some(&event));
        Some((at.x.floor() as i32, at.y.floor() as i32))
    }
}
