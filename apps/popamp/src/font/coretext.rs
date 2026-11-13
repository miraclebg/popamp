//! System fonts on macOS. CoreText comes later; for now every family is
//! one of the fonts every Mac has, and no fallbacks are looked for.

use std::path::{Path, PathBuf};

/// A font every Mac has, whatever `name` asks for.
pub fn family(_name: &str) -> Option<PathBuf> {
    ["/System/Library/Fonts/Geneva.ttf", "/System/Library/Fonts/Helvetica.ttc"]
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .map(Path::to_path_buf)
}

/// The fonts to try for `c`: none yet.
pub fn for_char(_c: char) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mac_always_has_a_sans_serif_font() {
        assert!(family("sans-serif").is_some_and(|p| p.is_file()));
    }
}
