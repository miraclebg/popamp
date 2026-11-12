//! The tray icon on macOS: a menu bar item, later. Not there yet: nothing
//! shows, whatever the setting says.

use super::{Sender, Snapshot};

/// No icon yet.
pub struct Tray;

impl Tray {
    pub fn new(_send: Sender) -> Tray {
        Tray
    }

    /// Nothing to show yet.
    pub fn update(&mut self, _snap: &Snapshot) {}
}
