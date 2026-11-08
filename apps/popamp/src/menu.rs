//! Pop-up menus: model, layout and drawing (windows live in `app.rs`).

use crate::dialogs::DialogKind;
use crate::font::Font;
use popamp_core::Command;
use popamp_skin::bitmap::{mix, rgb};
use popamp_skin::Bitmap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum MenuAction {
    Cmd(Command),
    Skin(Option<PathBuf>),
    VisMode(i32),
    /// Opens the visual plug-in with this id.
    #[cfg(feature = "visualizers")]
    Visualizer(String),
    SaStyle(i32),
    OscStyle(i32),
    SaPeaks,
    SaThick,
    EqBands(bool),
    EqLimiter,
    ReplayGain(Option<i32>),
    /// Window size in percent; 100 turns "Double Size" off.
    Zoom(i32),
    Scaling(popamp_core::config::Scaling),
    BarEdge(popamp_core::config::BarEdge),
    /// A bar's size in percent.
    BarZoom(i32),
    TitleFormat(String),
    EqPreset(usize, bool),
    EqDialog(DialogKind),
    PlaySelected,
    PlayBookmark(usize),
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub accel: String,
    pub action: Option<MenuAction>,
    pub checked: bool,
    pub sub: Vec<MenuItem>,
    pub separator: bool,
}

impl MenuItem {
    pub fn action(label: &str, accel: &str, action: MenuAction) -> Self {
        Self { label: label.into(), accel: accel.into(), action: Some(action), checked: false, sub: vec![], separator: false }
    }
    pub fn cmd(label: &str, accel: &str, c: Command) -> Self {
        Self::action(label, accel, MenuAction::Cmd(c))
    }
    pub fn sub(label: &str, items: Vec<MenuItem>) -> Self {
        Self { label: label.into(), accel: String::new(), action: None, checked: false, sub: items, separator: false }
    }
    pub fn label(label: &str) -> Self {
        Self { label: label.into(), accel: String::new(), action: None, checked: false, sub: vec![], separator: false }
    }
    pub fn sep() -> Self {
        Self { label: String::new(), accel: String::new(), action: None, checked: false, sub: vec![], separator: true }
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    pub fn enabled(&self) -> bool {
        !self.separator && (self.action.is_some() || !self.sub.is_empty())
    }
}

pub const FONT_PX: u32 = 12;

/// A tooltip: black text on pale yellow, as system tooltips used to be.
pub fn tooltip(text: &str, font: &mut Font) -> Bitmap {
    let (w, h) = (font.width(text, FONT_PX) + 6, 16);
    let mut b = Bitmap::new(w, h, rgb(255, 255, 225));
    b.bevel(0, 0, w, h, rgb(0, 0, 0), rgb(0, 0, 0));
    font.draw(&mut b, 3, 1, text, FONT_PX, rgb(0, 0, 0), 1, w - 2);
    b
}
const ITEM_H: i32 = 20;
const SEP_H: i32 = 7;
const PAD_X: i32 = 22;

pub const BG: u32 = rgb(0x24, 0x28, 0x33);
pub const BORDER: u32 = rgb(0x4a, 0x52, 0x68);
pub const TEXT: u32 = rgb(0xe6, 0xea, 0xf2);
pub const DIM: u32 = rgb(0x8a, 0x92, 0xa6);
pub const HILITE: u32 = rgb(0x3d, 0x5a, 0x99);

/// Geometry of one open menu level.
pub struct Layout {
    pub w: i32,
    pub h: i32,
    /// (y, height) per item.
    pub rows: Vec<(i32, i32)>,
}

pub fn layout(items: &[MenuItem], font: &mut Font) -> Layout {
    let mut y = 3;
    let mut rows = Vec::with_capacity(items.len());
    let mut label_w = 0;
    let mut accel_w = 0;
    for it in items {
        let h = if it.separator { SEP_H } else { ITEM_H };
        rows.push((y, h));
        y += h;
        if !it.separator {
            label_w = label_w.max(font.width(&it.label, FONT_PX));
            accel_w = accel_w.max(font.width(&it.accel, FONT_PX));
        }
    }
    let w = (PAD_X + label_w + if accel_w > 0 { 24 + accel_w } else { 0 } + 26).max(140);
    Layout { w, h: y + 3, rows }
}

pub fn hit(layout: &Layout, items: &[MenuItem], x: i32, y: i32) -> Option<usize> {
    if x < 0 || x >= layout.w {
        return None;
    }
    layout.rows.iter().position(|&(ry, rh)| y >= ry && y < ry + rh).filter(|&i| items[i].enabled())
}

pub fn draw(b: &mut Bitmap, items: &[MenuItem], layout: &Layout, hover: Option<usize>, font: &mut Font) {
    b.fill(0, 0, b.w, b.h, BG);
    b.bevel(0, 0, b.w, b.h, BORDER, mix(BORDER, BG, 0.6));
    for (i, it) in items.iter().enumerate() {
        let (y, h) = layout.rows[i];
        if it.separator {
            b.hline(6, y + h / 2, layout.w - 12, mix(BORDER, BG, 0.3));
            continue;
        }
        let enabled = it.enabled();
        if hover == Some(i) && enabled {
            b.fill(2, y, layout.w - 4, h, HILITE);
        }
        let color = if enabled { TEXT } else { DIM };
        let ty = y + (h - FONT_PX as i32) / 2 - 1;
        if it.checked {
            // Check mark.
            for k in 0..3 {
                b.set(7 + k, y + 10 + k, color);
                b.set(7 + k, y + 11 + k, color);
            }
            for k in 0..5 {
                b.set(10 + k, y + 12 - k, color);
                b.set(10 + k, y + 13 - k, color);
            }
        }
        font.draw(b, PAD_X, ty, &it.label, FONT_PX, color, 0, layout.w);
        if !it.accel.is_empty() {
            let aw = font.width(&it.accel, FONT_PX);
            font.draw(b, layout.w - 14 - aw, ty, &it.accel, FONT_PX, DIM, 0, layout.w);
        }
        if !it.sub.is_empty() {
            let (ax, ay) = (layout.w - 10, y + h / 2);
            for k in 0..4 {
                b.vline(ax + k, ay - (3 - k), (3 - k) * 2 + 1, color);
            }
        }
    }
}
