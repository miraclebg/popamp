//! X11: window types through winit, shapes, struts and work areas through
//! a connection of our own.

use super::{shape_rects, Mask, WindowRole};
use crate::bar::{Screen, Strip};
use popamp_core::config::BarEdge;
use winit::platform::x11::{WindowAttributesExtX11, WindowType};
use winit::window::{Window, WindowAttributes};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{AtomEnum, ClipOrdering, ConnectionExt as _, PropMode, Rectangle};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Fixed-size window attributes for `role`, borderless but for dialogs.
pub fn attributes(role: WindowRole) -> WindowAttributes {
    let attrs = Window::default_attributes().with_decorations(false).with_resizable(false).with_name("popamp", "Popamp");
    match role {
        WindowRole::Main => attrs,
        WindowRole::Docked => attrs.with_x11_window_type(vec![WindowType::Utility]),
        WindowRole::Bar => attrs.with_x11_window_type(vec![WindowType::Dock]),
        WindowRole::Popup => attrs.with_override_redirect(true).with_x11_window_type(vec![WindowType::PopupMenu]),
        WindowRole::Tooltip => attrs.with_override_redirect(true).with_active(false).with_x11_window_type(vec![WindowType::Tooltip]),
        WindowRole::Dialog => attrs.with_decorations(true).with_x11_window_type(vec![WindowType::Dialog]),
    }
}

/// Window managers differ in whether they minimize transient windows with
/// their owner, so every window is minimized and restored by itself.
pub const OWNED_FOLLOW_OWNER: bool = false;

/// Each window is minimized as the owner rule above says.
pub const MINIMIZE_HIDES_COMPANIONS: bool = false;

/// A connection to the X server for what winit does not cover.
pub struct Platform {
    x11: Option<RustConnection>,
}

impl Platform {
    pub fn connect() -> Platform {
        let x11 = x11rb::connect(None).map(|(c, _)| c).map_err(|e| log::warn!("no X11 connection for window shapes: {e}")).ok();
        Platform { x11 }
    }

    /// Cuts `window` to the `region.txt` polygons (`None` restores the
    /// rectangle). `size` is in skin pixels, drawn at `scale`.
    /// Shows only the pixels `mask` covers of a window `size` screen pixels
    /// big; all of it without a mask.
    pub fn set_shape(&self, window: &Window, mask: Option<&Mask>, size: (i32, i32)) -> Result<(), String> {
        let (Some(conn), Some(xid)) = (&self.x11, x11_window_id(window)) else { return Ok(()) };
        let err = |e: &dyn std::fmt::Display| e.to_string();
        if conn.extension_information(shape::X11_EXTENSION_NAME).map_err(|e| err(&e))?.is_none() {
            return Err("X server has no SHAPE extension".into());
        }
        let rects: Vec<Rectangle> =
            shape_rects(mask, size).into_iter().map(|(x, y, width, height)| Rectangle { x, y, width, height }).collect();
        conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, xid, 0, 0, &rects).map_err(|e| err(&e))?;
        conn.flush().map_err(|e| err(&e))?;
        Ok(())
    }

    /// Reserves `strip` for the bar `window` (`_NET_WM_STRUT_PARTIAL`).
    pub fn reserve(&self, window: &Window, strip: Strip) -> Result<(), String> {
        let (Some(conn), Some(xid), Some(root_h)) = (&self.x11, x11_window_id(window), self.root_height()) else { return Ok(()) };
        let strut = strut(strip, root_h);
        let err = |e: &dyn std::fmt::Display| e.to_string();
        let atom = |name: &str| -> Result<u32, String> {
            Ok(conn.intern_atom(false, name.as_bytes()).map_err(|e| err(&e))?.reply().map_err(|e| err(&e))?.atom)
        };
        // The old four-number form for window managers without the partial one.
        for (name, values) in [("_NET_WM_STRUT_PARTIAL", &strut[..]), ("_NET_WM_STRUT", &strut[..4])] {
            conn.change_property32(PropMode::REPLACE, xid, atom(name)?, AtomEnum::CARDINAL, values).map_err(|e| err(&e))?;
        }
        conn.flush().map_err(|e| err(&e))?;
        Ok(())
    }

    /// Makes `window` a companion of `owner` (`WM_TRANSIENT_FOR`): the
    /// window manager then keeps it above the owner and brings it up and
    /// minimizes it together with the owner, as when the owner's taskbar
    /// icon is clicked. `None` makes it a window of its own again.
    pub fn attach(&self, window: &Window, owner: Option<&Window>) -> Result<(), String> {
        let (Some(conn), Some(xid)) = (&self.x11, x11_window_id(window)) else { return Ok(()) };
        let err = |e: &dyn std::fmt::Display| e.to_string();
        match owner.and_then(x11_window_id) {
            Some(owner) => conn.change_property32(PropMode::REPLACE, xid, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::WINDOW, &[owner]),
            None => conn.delete_property(xid, AtomEnum::WM_TRANSIENT_FOR.into()),
        }
        .map_err(|e| err(&e))?;
        conn.flush().map_err(|e| err(&e))
    }

    /// Nothing to do: menus and tooltips are override-redirect windows,
    /// which the window manager never focuses.
    pub fn no_activate(&self, _window: &Window) {}

    /// Nothing to follow: the system cuts the window and passes clicks.
    pub fn tracks_cursor(&self) -> bool {
        false
    }

    /// Nothing to do: the system passes clicks through a shape.
    pub fn track_cursor(&self, _windows: &[&Window], _button_down: bool) {}

    /// Nothing to do: owned windows come up with their owner.
    pub fn raise_together(&self, _windows: &[&Window]) {}

    /// Nothing to give back: a strut goes with its window.
    pub fn release(&self, _window: &Window) {}

    /// The work areas the window manager publishes for the current desktop:
    /// one per monitor where it says so (`_GTK_WORKAREAS_Dn`), else one
    /// for all of them (`_NET_WORKAREA`).
    pub fn work_areas(&self) -> Vec<Screen> {
        let Some(conn) = &self.x11 else { return Vec::new() };
        let Some(root) = conn.setup().roots.first().map(|r| r.root) else { return Vec::new() };
        let cardinals = |name: &str| -> Option<Vec<u32>> {
            let atom = conn.intern_atom(true, name.as_bytes()).ok()?.reply().ok()?.atom;
            let reply = conn.get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1024).ok()?.reply().ok()?;
            let values = reply.value32()?.collect();
            Some(values)
        };
        let desktop = cardinals("_NET_CURRENT_DESKTOP").and_then(|d| d.first().copied()).unwrap_or(0) as usize;
        let rects = |v: &[u32]| -> Vec<Screen> {
            v.as_chunks::<4>().0.iter().map(|r| Screen { x: r[0] as i32, y: r[1] as i32, w: r[2] as i32, h: r[3] as i32 }).collect()
        };
        match cardinals(&format!("_GTK_WORKAREAS_D{desktop}")) {
            Some(per_monitor) if !per_monitor.is_empty() => rects(&per_monitor),
            _ => cardinals("_NET_WORKAREA").map(|all| rects(&all).into_iter().skip(desktop).take(1).collect()).unwrap_or_default(),
        }
    }

    /// The height of the X screen all monitors are part of.
    fn root_height(&self) -> Option<i32> {
        Some(self.x11.as_ref()?.setup().roots.first()?.height_in_pixels as i32)
    }
}

fn x11_window_id(w: &Window) -> Option<u32> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(h) => Some(h.window as u32),
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None,
    }
}

/// `_NET_WM_STRUT_PARTIAL` for `strip`: what it takes from the root window,
/// `root_h` pixels high.
fn strut(strip: Strip, root_h: i32) -> [u32; 12] {
    let Screen { x, y, w, h } = strip.area();
    let (start, end) = (x as u32, (x + w - 1) as u32);
    match strip.edge {
        BarEdge::Top => [0, 0, (y + h) as u32, 0, 0, 0, 0, 0, start, end, 0, 0],
        BarEdge::Bottom => [0, 0, 0, (root_h - y) as u32, 0, 0, 0, 0, 0, 0, start, end],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struts_count_from_the_root_window_edges() {
        // The right monitor is shorter than the 1080-pixel root window.
        let screen = Screen { x: 1920, y: 0, w: 1280, h: 1024 };
        let bottom = strut(Strip { edge: BarEdge::Bottom, screen, height: 28 }, 1080);
        assert_eq!(bottom, [0, 0, 0, 56 + 28, 0, 0, 0, 0, 0, 0, 1920, 3199]);
        let top = strut(Strip { edge: BarEdge::Top, screen, height: 28 }, 1080);
        assert_eq!(top, [0, 0, 28, 0, 0, 0, 0, 0, 1920, 3199, 0, 0]);
    }
}
