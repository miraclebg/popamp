//! Popamp's keys on a Mac, Mac first: ⌘ does what Ctrl does on Linux and
//! Windows, and the shortcuts macOS or Mac habits already use move aside
//! (⌘W closes, ⌘V pastes, ⌫ deletes, ⇧⌘3 is the system's). Each Mac
//! combination becomes the one the key handler already knows, so there is
//! one list of shortcuts; the same table turns menu labels into Mac ones.
//! The Control key is not the main modifier here: Ctrl+click is a right
//! click, and Ctrl+letter does nothing.

use popamp_skin::layout::WindowKey;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// The modifiers held: ⌘, ⌃, ⌥ and ⇧.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mods {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// What a Mac key press means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mac {
    /// The combination the key handler knows from Linux and Windows.
    Key { code: KeyCode, ctrl: bool, alt: bool, shift: bool },
    /// ⌘V: copied links or files into the playlist.
    Paste,
    /// ⌘W: as the window's close button.
    Close,
    /// ⌘M.
    Minimize,
    /// A key that does nothing on a Mac.
    Nothing,
}

/// The meaning of `code` pressed with `m` in `window`.
pub fn translate(code: KeyCode, m: Mods, window: WindowKey) -> Mac {
    use KeyCode as K;
    let key = |code, ctrl, alt, shift| Mac::Key { code, ctrl, alt, shift };
    if m.ctrl && !m.cmd {
        return Mac::Nothing;
    }
    let playlist = window == WindowKey::Playlist;
    match (code, m.cmd, m.alt, m.shift) {
        (K::Comma, true, false, false) => key(K::KeyP, true, false, false),
        (K::KeyW, true, false, false) => Mac::Close,
        (K::KeyW, true, false, true) => key(K::KeyW, true, false, false),
        (K::KeyM, true, false, false) => Mac::Minimize,
        (K::Backquote, true, false, false) => key(K::Tab, true, false, false),
        (K::KeyA, true, true, false) => key(K::KeyA, true, playlist, false),
        (K::KeyA, true, false, false) if playlist => key(K::KeyA, true, false, false),
        (K::KeyA, true, false, false) => Mac::Nothing,
        (K::KeyI, true, false, false) => key(K::Digit3, false, true, false),
        (K::KeyI, true, false, true) => key(K::KeyI, true, false, false),
        (K::Digit1 | K::Digit2 | K::Digit3, true, true, false) => key(code, true, false, true),
        (K::Digit1 | K::Digit2 | K::Digit3, true, false, true) => Mac::Nothing,
        (K::KeyV, true, true, false) => key(K::KeyV, true, false, false),
        (K::KeyV, true, false, false) => Mac::Paste,
        (K::Backspace, cmd, alt, shift) => key(K::Delete, cmd, alt, shift),
        (K::ArrowUp, true, false, false) => key(K::Home, false, false, false),
        (K::ArrowDown, true, false, false) => key(K::End, false, false, false),
        (code, cmd, alt, shift) => key(code, cmd, alt, shift),
    }
}

/// The button a click with `m` means: Ctrl+click is a right click.
pub fn button(b: MouseButton, m: Mods) -> MouseButton {
    match b {
        MouseButton::Left if m.ctrl && !m.cmd => MouseButton::Right,
        other => other,
    }
}

/// A Linux and Windows shortcut label (`Ctrl+Shift+1`) as the Mac's
/// (`⌥⌘1`): the table's exceptions first, then ⌥⇧⌘ before the key.
pub fn label(accel: &str) -> String {
    let moved = match accel {
        "Ctrl+P" => Some("⌘,"),
        "Ctrl+W" => Some("⇧⌘W"),
        "Ctrl+A" | "Ctrl+Alt+A" => Some("⌥⌘A"),
        "Alt+3" => Some("⌘I"),
        "Ctrl+I" => Some("⇧⌘I"),
        "Ctrl+V" => Some("⌥⌘V"),
        "Ctrl+Tab" => Some("⌘`"),
        "Alt+F4" => Some("⌘Q"),
        _ => None,
    };
    if let Some(mac) = moved {
        return mac.into();
    }
    let mut parts: Vec<&str> = accel.split('+').collect();
    let Some(key) = parts.pop() else { return accel.into() };
    let has = |m: &str| parts.contains(&m);
    let (ctrl, alt, mut shift) = (has("Ctrl"), has("Alt"), has("Shift"));
    let mut alt = alt;
    // Ctrl+Shift+digit sorts the list; on a Mac it is ⌥⌘digit.
    if ctrl && shift && key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) {
        (alt, shift) = (true, false);
    }
    let key = if key == "Del" { "⌫" } else { key };
    let mut out = String::new();
    for (on, sign) in [(alt, "⌥"), (shift, "⇧"), (ctrl, "⌘")] {
        if on {
            out.push_str(sign);
        }
    }
    out + key
}

/// What ⌘V adds to the playlist: the copied files if there are any, else
/// each line of copied text that is a web address or names a file or
/// folder that is there. Anything else (a sentence, an empty clipboard)
/// adds nothing.
pub fn paste_items(files: Vec<String>, text: Option<String>) -> Vec<String> {
    if !files.is_empty() {
        return files;
    }
    // Only whole paths: a word such as "Library" names a folder relative to
    // `/`, where an app started from Finder runs.
    let usable = |l: &&str| {
        let path = std::path::Path::new(l);
        l.starts_with("http://") || l.starts_with("https://") || (path.is_absolute() && path.exists())
    };
    text.map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).filter(usable).map(str::to_string).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, ctrl: bool, alt: bool, shift: bool) -> Mac {
        Mac::Key { code, ctrl, alt, shift }
    }
    const CMD: Mods = Mods { cmd: true, ctrl: false, alt: false, shift: false };
    const NONE: Mods = Mods { cmd: false, ctrl: false, alt: false, shift: false };

    #[test]
    fn command_does_what_ctrl_does_elsewhere() {
        for code in [KeyCode::KeyJ, KeyCode::KeyL, KeyCode::KeyD, KeyCode::KeyT, KeyCode::KeyK] {
            assert_eq!(translate(code, CMD, WindowKey::Main), key(code, true, false, false), "{code:?}");
        }
        assert_eq!(translate(KeyCode::KeyW, Mods { alt: true, ..NONE }, WindowKey::Main), key(KeyCode::KeyW, false, true, false));
        assert_eq!(translate(KeyCode::KeyX, NONE, WindowKey::Main), key(KeyCode::KeyX, false, false, false));
    }

    #[test]
    fn the_mac_table() {
        let m = WindowKey::Main;
        let pl = WindowKey::Playlist;
        let with = |cmd, alt, shift| Mods { cmd, ctrl: false, alt, shift };
        assert_eq!(translate(KeyCode::Comma, CMD, m), key(KeyCode::KeyP, true, false, false));
        assert_eq!(translate(KeyCode::KeyW, CMD, m), Mac::Close);
        assert_eq!(translate(KeyCode::KeyM, CMD, m), Mac::Minimize);
        assert_eq!(translate(KeyCode::KeyW, with(true, false, true), m), key(KeyCode::KeyW, true, false, false));
        assert_eq!(translate(KeyCode::Backquote, CMD, m), key(KeyCode::Tab, true, false, false));
        assert_eq!(translate(KeyCode::KeyA, with(true, true, false), m), key(KeyCode::KeyA, true, false, false));
        assert_eq!(translate(KeyCode::KeyA, with(true, true, false), pl), key(KeyCode::KeyA, true, true, false));
        assert_eq!(translate(KeyCode::KeyA, CMD, pl), key(KeyCode::KeyA, true, false, false), "select all");
        assert_eq!(translate(KeyCode::KeyA, CMD, m), Mac::Nothing);
        assert_eq!(translate(KeyCode::KeyI, CMD, m), key(KeyCode::Digit3, false, true, false));
        assert_eq!(translate(KeyCode::KeyI, with(true, false, true), pl), key(KeyCode::KeyI, true, false, false));
        assert_eq!(translate(KeyCode::Digit2, with(true, true, false), pl), key(KeyCode::Digit2, true, false, true));
        assert_eq!(translate(KeyCode::KeyV, with(true, true, false), m), key(KeyCode::KeyV, true, false, false));
        assert_eq!(translate(KeyCode::KeyV, CMD, m), Mac::Paste);
        assert_eq!(translate(KeyCode::Backspace, NONE, pl), key(KeyCode::Delete, false, false, false));
        assert_eq!(translate(KeyCode::Backspace, CMD, pl), key(KeyCode::Delete, true, false, false));
        assert_eq!(translate(KeyCode::Backspace, with(true, false, true), pl), key(KeyCode::Delete, true, false, true));
        assert_eq!(translate(KeyCode::Backspace, Mods { alt: true, ..NONE }, pl), key(KeyCode::Delete, false, true, false));
        assert_eq!(translate(KeyCode::ArrowUp, CMD, pl), key(KeyCode::Home, false, false, false));
        assert_eq!(translate(KeyCode::ArrowDown, CMD, pl), key(KeyCode::End, false, false, false));
    }

    #[test]
    fn control_is_not_the_main_modifier() {
        assert_eq!(translate(KeyCode::KeyJ, Mods { ctrl: true, ..NONE }, WindowKey::Main), Mac::Nothing);
    }

    #[test]
    fn control_click_is_a_right_click() {
        assert_eq!(button(MouseButton::Left, Mods { ctrl: true, ..NONE }), MouseButton::Right);
        assert_eq!(button(MouseButton::Left, CMD), MouseButton::Left);
    }

    #[test]
    fn menus_show_mac_shortcuts() {
        for (linux, mac) in [
            ("Ctrl+J", "⌘J"),
            ("Ctrl+P", "⌘,"),
            ("Ctrl+W", "⇧⌘W"),
            ("Ctrl+A", "⌥⌘A"),
            ("Alt+3", "⌘I"),
            ("Ctrl+I", "⇧⌘I"),
            ("Ctrl+Shift+1", "⌥⌘1"),
            ("Ctrl+V", "⌥⌘V"),
            ("Del", "⌫"),
            ("Ctrl+Del", "⌘⌫"),
            ("Ctrl+Shift+Del", "⇧⌘⌫"),
            ("Alt+Del", "⌥⌫"),
            ("Alt+F4", "⌘Q"),
            ("Ctrl+Click", "⌘Click"),
            ("Shift+Click", "⇧Click"),
            ("Alt+W", "⌥W"),
            ("Ctrl+Alt+I", "⌥⌘I"),
            ("Shift+R", "⇧R"),
            ("R", "R"),
        ] {
            assert_eq!(label(linux), mac, "{linux}");
        }
    }

    #[test]
    fn paste_takes_files_then_links_and_paths_and_nothing_else() {
        let tmp = std::env::temp_dir();
        let dir = tmp.to_string_lossy().into_owned();
        assert_eq!(paste_items(vec!["/a.mp3".into()], Some("https://x".into())), ["/a.mp3"]);
        let text = format!("https://ice1.somafm.com/groovesalad-128-mp3\n\n  {dir}  \nhello there\n/popamp/none.mp3\nftp://x");
        assert_eq!(paste_items(vec![], Some(text)), ["https://ice1.somafm.com/groovesalad-128-mp3".to_string(), dir]);
        assert!(paste_items(vec![], None).is_empty());
    }

    // An app started from Finder runs in `/`, where "Library" or "Users"
    // are folders: a copied word must not add one.
    #[test]
    fn a_copied_word_is_not_a_path() {
        assert!(paste_items(vec![], Some("src".into())).is_empty(), "src exists beside the crate's tests");
    }
}
