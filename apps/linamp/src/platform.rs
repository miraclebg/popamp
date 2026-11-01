//! Everything that depends on the windowing system. Linamp runs on X11
//! (or Xwayland); another backend would replace this module only.

use linamp_skin::textcfg::region_mask;
use winit::platform::x11::{WindowAttributesExtX11, WindowType};
use winit::window::{Window, WindowAttributes};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{ClipOrdering, Rectangle};
use x11rb::rust_connection::RustConnection;

/// What a window is for, which decides how the window manager treats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRole {
    /// The main window: in the taskbar.
    Main,
    /// Equalizer and playlist: tool windows docked to the main one.
    Docked,
    /// Pop-up menus: placed and stacked by us, not the window manager.
    Popup,
}

/// Borderless, fixed-size window attributes for `role`.
pub fn attributes(role: WindowRole) -> WindowAttributes {
    let attrs = Window::default_attributes().with_decorations(false).with_resizable(false).with_name("linamp", "Linamp");
    match role {
        WindowRole::Main => attrs,
        WindowRole::Docked => attrs.with_x11_window_type(vec![WindowType::Utility]),
        WindowRole::Popup => attrs.with_override_redirect(true).with_x11_window_type(vec![WindowType::PopupMenu]),
    }
}

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
    /// rectangle). `size` is in skin pixels, drawn `scale` times larger.
    pub fn set_shape(&self, window: &Window, polys: Option<&[Vec<(i32, i32)>]>, size: (i32, i32), scale: i32) -> Result<(), String> {
        let (Some(conn), Some(xid)) = (&self.x11, x11_window_id(window)) else { return Ok(()) };
        let err = |e: &dyn std::fmt::Display| e.to_string();
        if conn.extension_information(shape::X11_EXTENSION_NAME).map_err(|e| err(&e))?.is_none() {
            return Err("X server has no SHAPE extension".into());
        }
        let rects: Vec<Rectangle> =
            shape_rects(polys, size, scale).into_iter().map(|(x, y, width, height)| Rectangle { x, y, width, height }).collect();
        conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, xid, 0, 0, &rects).map_err(|e| err(&e))?;
        conn.flush().map_err(|e| err(&e))?;
        Ok(())
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

/// The visible part of a window as `(x, y, w, h)` rectangles in screen
/// pixels: one per horizontal run of the region mask.
fn shape_rects(polys: Option<&[Vec<(i32, i32)>]>, size: (i32, i32), scale: i32) -> Vec<(i16, i16, u16, u16)> {
    let (w, h) = size;
    let whole = vec![(0, 0, (w * scale) as u16, (h * scale) as u16)];
    let Some(polys) = polys else { return whole };
    let mask = region_mask(polys, w, h);
    let mut out = Vec::new();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if !mask[(y * w + x) as usize] {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && mask[(y * w + x) as usize] {
                x += 1;
            }
            out.push(((start * scale) as i16, (y * scale) as i16, ((x - start) * scale) as u16, scale as u16));
        }
    }
    // A region that hides everything is a broken skin: ignore it.
    if out.is_empty() {
        whole
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_region_is_the_whole_window() {
        assert_eq!(shape_rects(None, (275, 116), 2), [(0, 0, 550, 232)]);
    }

    #[test]
    fn region_becomes_scaled_row_runs() {
        // A 4x2 rectangle at (1, 0) inside a 6x3 window.
        let rect = vec![vec![(1, 0), (5, 0), (5, 2), (1, 2)]];
        assert_eq!(shape_rects(Some(&rect), (6, 3), 2), [(2, 0, 8, 2), (2, 2, 8, 2)]);
    }

    #[test]
    fn empty_region_is_ignored() {
        let nothing = vec![vec![(0, 0), (0, 0), (0, 0)]];
        assert_eq!(shape_rects(Some(&nothing), (10, 4), 1), [(0, 0, 10, 4)]);
    }
}
