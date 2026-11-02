//! Everything that depends on the windowing system. Popamp runs on X11
//! (or Xwayland); another backend would replace this module only.

use crate::scale::Scale;
use winit::platform::x11::{WindowAttributesExtX11, WindowType};
use winit::window::{Window, WindowAttributes};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{AtomEnum, ClipOrdering, ConnectionExt as _, PropMode, Rectangle};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// What a window is for, which decides how the window manager treats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRole {
    /// The main window: in the taskbar.
    Main,
    /// Equalizer and playlist: tool windows docked to the main one.
    Docked,
    /// A bar skin's main window: a panel at a screen edge.
    Bar,
    /// Pop-up menus: placed and stacked by us, not the window manager.
    Popup,
    /// Tooltips: like menus, but never focused.
    Tooltip,
    /// Popamp's own dialogs: ordinary windows with a title bar.
    Dialog,
}

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

    /// Reserves the strip a bar takes (`_NET_WM_STRUT_PARTIAL`, see
    /// [`crate::bar::strut`]) for `window`.
    pub fn set_strut(&self, window: &Window, strut: [u32; 12]) -> Result<(), String> {
        let (Some(conn), Some(xid)) = (&self.x11, x11_window_id(window)) else { return Ok(()) };
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

    /// The work areas the window manager publishes for the current desktop:
    /// one per monitor where it says so (`_GTK_WORKAREAS_Dn`), else one
    /// for all of them (`_NET_WORKAREA`).
    pub fn work_areas(&self) -> Vec<crate::bar::Screen> {
        let Some(conn) = &self.x11 else { return Vec::new() };
        let Some(root) = conn.setup().roots.first().map(|r| r.root) else { return Vec::new() };
        let cardinals = |name: &str| -> Option<Vec<u32>> {
            let atom = conn.intern_atom(true, name.as_bytes()).ok()?.reply().ok()?.atom;
            let reply = conn.get_property(false, root, atom, AtomEnum::CARDINAL, 0, 1024).ok()?.reply().ok()?;
            let values = reply.value32()?.collect();
            Some(values)
        };
        let desktop = cardinals("_NET_CURRENT_DESKTOP").and_then(|d| d.first().copied()).unwrap_or(0) as usize;
        let rects = |v: &[u32]| -> Vec<crate::bar::Screen> {
            v.as_chunks::<4>()
                .0
                .iter()
                .map(|r| crate::bar::Screen { x: r[0] as i32, y: r[1] as i32, w: r[2] as i32, h: r[3] as i32 })
                .collect()
        };
        match cardinals(&format!("_GTK_WORKAREAS_D{desktop}")) {
            Some(per_monitor) if !per_monitor.is_empty() => rects(&per_monitor),
            _ => cardinals("_NET_WORKAREA").map(|all| rects(&all).into_iter().skip(desktop).take(1).collect()).unwrap_or_default(),
        }
    }

    /// The height of the X screen all monitors are part of.
    pub fn root_height(&self) -> Option<i32> {
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

/// Which pixels of a window show: `w` x `h` of them, each `scale` times
/// bigger on the screen.
pub struct Mask {
    pub w: i32,
    pub h: i32,
    pub on: Vec<bool>,
    pub scale: Scale,
}

/// The visible part of a window as `(x, y, w, h)` rectangles in screen
/// pixels: one per horizontal run of the mask.
fn shape_rects(mask: Option<&Mask>, size: (i32, i32)) -> Vec<(i16, i16, u16, u16)> {
    let whole = vec![(0, 0, size.0 as u16, size.1 as u16)];
    let Some(Mask { w, h, on: mask, scale }) = mask else { return whole };
    let (w, h, scale) = (*w, *h, *scale);
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
            let (x0, y0) = (scale.px(start), scale.px(y));
            out.push((x0 as i16, y0 as i16, (scale.px(x) - x0) as u16, (scale.px(y + 1) - y0) as u16));
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
    use popamp_core::config::Scaling;

    fn region(polys: &[Vec<(i32, i32)>], w: i32, h: i32, scale: Scale) -> Mask {
        Mask { w, h, on: popamp_skin::textcfg::region_mask(polys, w, h), scale }
    }

    #[test]
    fn no_mask_is_the_whole_window() {
        assert_eq!(shape_rects(None, (550, 232)), [(0, 0, 550, 232)]);
    }

    #[test]
    fn masks_become_scaled_row_runs() {
        // A 4x2 rectangle at (1, 0) inside a 6x3 window.
        let rect = vec![vec![(1, 0), (5, 0), (5, 2), (1, 2)]];
        let x2 = Scale::new(200, Scaling::Sharp);
        assert_eq!(shape_rects(Some(&region(&rect, 6, 3, x2)), (12, 6)), [(2, 0, 8, 2), (2, 2, 8, 2)]);
        // At x1.5 the runs meet without gaps: rows 0..2 and 2..3.
        let x15 = Scale::new(150, Scaling::Sharp);
        assert_eq!(shape_rects(Some(&region(&rect, 6, 3, x15)), (9, 5)), [(2, 0, 6, 2), (2, 2, 6, 1)]);
    }

    #[test]
    fn empty_masks_are_ignored() {
        let nothing = vec![vec![(0, 0), (0, 0), (0, 0)]];
        assert_eq!(shape_rects(Some(&region(&nothing, 10, 4, Scale::ONE)), (10, 4)), [(0, 0, 10, 4)]);
    }
}
