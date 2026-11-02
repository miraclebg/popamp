//! Types shared by the skinned windows.

use crate::menu::MenuItem;

/// A mouse event in window-logical (1x) coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Input {
    pub x: i32,
    pub y: i32,
    pub right: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub double: bool,
}

/// What a window wants the window manager layer to do after an event.
#[derive(Debug)]
pub enum Out {
    None,
    Redraw,
    /// Keep sending motion/up events to this window.
    Capture,
    /// Start dragging the window (and anything docked to it).
    BeginMove,
    /// Start resizing (playlist grip).
    BeginResize,
    Menu(Vec<MenuItem>),
}
