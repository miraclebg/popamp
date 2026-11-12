//! Everything that depends on the windowing system: X11 (or Xwayland) on
//! Linux, Win32 on Windows, AppKit on macOS. The rest of the program sees
//! only this module.

use crate::bar::Screen;
use crate::scale::Scale;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize, Position, Size};
use winit::monitor::MonitorHandle;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{attributes, use_parent_console, Platform, OWNED_FOLLOW_OWNER};
#[cfg(linux_like)]
mod x11;
#[cfg(linux_like)]
pub use x11::{attributes, Platform, OWNED_FOLLOW_OWNER};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{attributes, Platform, OWNED_FOLLOW_OWNER};

/// Whether the desktop counts in points rather than screen pixels. macOS
/// lays its screens out in points, two pixels each on a Retina screen, so a
/// pixel position means something else on each screen there; window
/// positions and sizes go to it in points, and each window draws at its own
/// screen's pixels per point. X11 and Windows have one grid of pixels.
pub const DESKTOP_IN_POINTS: bool = cfg!(target_os = "macos");

/// A window position in the desktop's units.
pub fn position(x: i32, y: i32) -> Position {
    if DESKTOP_IN_POINTS {
        LogicalPosition::new(x as f64, y as f64).into()
    } else {
        PhysicalPosition::new(x, y).into()
    }
}

/// A window size in the desktop's units.
pub fn size(w: i32, h: i32) -> Size {
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    if DESKTOP_IN_POINTS {
        LogicalSize::new(w as f64, h as f64).into()
    } else {
        PhysicalSize::new(w, h).into()
    }
}

/// The size of a window drawn `w` x `h` screen pixels big at `pct` (a menu,
/// a tooltip, a dialog drawn at the display scaling), in the desktop's units.
pub fn drawn_size(w: i32, h: i32, pct: i32) -> Size {
    if DESKTOP_IN_POINTS {
        let pt = |v: i32| (v * 100 + pct - 1) / pct.max(1);
        size(pt(w), pt(h))
    } else {
        size(w, h)
    }
}

/// A point winit gives in screen pixels (a window's position, the cursor in
/// it), in the desktop's units; `factor` is the window's pixels per point.
pub fn from_screen(x: f64, y: f64, factor: f64) -> (i32, i32) {
    let f = if DESKTOP_IN_POINTS && factor > 0.0 { factor } else { 1.0 };
    ((x / f).floor() as i32, (y / f).floor() as i32)
}

/// A monitor's rectangle in the desktop's units.
pub fn monitor(m: &MonitorHandle) -> Screen {
    let (p, s) = (m.position(), m.size());
    let f = if DESKTOP_IN_POINTS { m.scale_factor() } else { 1.0 };
    let unit = |v: f64| (v / f).round() as i32;
    Screen { x: unit(p.x as f64), y: unit(p.y as f64), w: unit(s.width as f64), h: unit(s.height as f64) }
}

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
#[cfg_attr(target_os = "macos", allow(dead_code))]
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

    #[cfg(target_os = "macos")]
    #[test]
    fn a_mac_desktop_counts_in_points() {
        assert_eq!(from_screen(301.0, 99.0, 2.0), (150, 49));
        assert!(matches!(position(10, 20), Position::Logical(p) if (p.x, p.y) == (10.0, 20.0)));
        assert!(matches!(size(275, 116), Size::Logical(s) if (s.width, s.height) == (275.0, 116.0)));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn elsewhere_the_desktop_counts_in_pixels() {
        assert_eq!(from_screen(301.0, 99.0, 2.0), (301, 99));
        assert!(matches!(position(10, 20), Position::Physical(p) if (p.x, p.y) == (10, 20)));
        assert!(matches!(size(275, 116), Size::Physical(s) if (s.width, s.height) == (275, 116)));
    }
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
