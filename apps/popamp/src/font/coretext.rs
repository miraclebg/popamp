//! System fonts on macOS through CoreText: the file of a family, and the
//! fonts that have a character the font in use lacks.

use objc2_core_foundation::{CFRange, CFRetained, CFString, CFURL};
use objc2_core_text::{kCTFontURLAttribute, CTFont};
use std::path::PathBuf;

/// The Mac font a generic family name stands for; other names as they are.
fn mac_family(name: &str) -> &str {
    match name {
        "sans-serif" => "Helvetica Neue",
        "serif" => "Times New Roman",
        "monospace" => "Menlo",
        other => other,
    }
}

/// The font CoreText gives for `name`, or what it puts in its place.
fn font(name: &str) -> CFRetained<CTFont> {
    // SAFETY: a null matrix asks for the identity transform.
    unsafe { CTFont::with_name(&CFString::from_str(mac_family(name)), 12.0, std::ptr::null()) }
}

/// The file `font` is read from.
fn file_of(font: &CTFont) -> Option<PathBuf> {
    // SAFETY: `kCTFontURLAttribute` is a font attribute; its value is a CFURL.
    let url = unsafe { font.attribute(kCTFontURLAttribute) }?.downcast::<CFURL>().ok()?;
    url.to_file_path().filter(|p| p.is_file())
}

/// The file of the font family `name`, or of what CoreText puts in its
/// place (generic names such as `sans-serif` included).
pub fn family(name: &str) -> Option<PathBuf> {
    file_of(&font(name))
}

/// The fonts to try for `c`, best first: the one CoreText's fallback picks.
pub fn for_char(c: char) -> Vec<PathBuf> {
    let mut buf = [0u8; 4];
    let text = CFString::from_str(c.encode_utf8(&mut buf));
    let range = CFRange { location: 0, length: c.len_utf16() as isize };
    // SAFETY: `range` lies within `text`.
    let fallback = unsafe { font("sans-serif").for_string(&text, range) };
    file_of(&fallback).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the font file at `path` draws `c`, as Popamp will read it.
    fn draws(path: &std::path::Path, c: char) -> bool {
        let face = fontdue::Font::from_bytes(std::fs::read(path).unwrap(), fontdue::FontSettings::default()).unwrap();
        face.lookup_glyph_index(c) != 0
    }

    #[test]
    fn generic_families_are_the_macs_own_fonts() {
        for name in ["sans-serif", "serif", "monospace"] {
            let path = family(name).unwrap_or_else(|| panic!("{name}"));
            assert!(draws(&path, 'A'), "{name}: {}", path.display());
        }
        assert_ne!(family("monospace"), family("sans-serif"), "each generic name is a font of its kind");
    }

    #[test]
    fn a_character_the_font_lacks_comes_from_a_font_that_has_it() {
        for c in ['Ж', '中'] {
            assert!(for_char(c).iter().any(|p| draws(p, c)), "{c}: {:?}", for_char(c));
        }
    }
}
