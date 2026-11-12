//! Dialogs on macOS, through `osascript`. Not there yet: every dialog
//! comes back cancelled, so a question is answered "no".

use super::Request;
use std::sync::Once;

/// Shows nothing yet; the chosen values: none.
pub fn execute(_req: &Request) -> Vec<String> {
    static SAID: Once = Once::new();
    SAID.call_once(|| log::warn!("dialogs are not there on macOS yet"));
    Vec::new()
}
