//! TrueType text for the playlist, menus and vector skins (fontdue).
//! Characters the font lacks come from a system font that has them,
//! found through fontconfig, or the registry on Windows.

use popamp_skin::bitmap::mix;
use popamp_skin::{Bitmap, Skin, TextStyle};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(unix)]
mod fontconfig;
#[cfg(unix)]
use fontconfig as system;
#[cfg(any(windows, test))]
mod windows;
#[cfg(windows)]
use windows as system;

/// The fonts the windows draw with.
pub struct Fonts {
    /// The playlist's, also used for menus and dialogs.
    pub list: Font,
    /// A vector skin's small texts.
    pub text: Option<Font>,
    /// A vector skin's play time.
    pub clock: Option<Font>,
}

impl Fonts {
    pub fn for_skin(skin: &Skin) -> Fonts {
        let of = |data: Option<&Vec<u8>>, family: &str| data.map_or_else(|| Font::load(family), |d| Font::from_data(d));
        let style = |s: &Option<TextStyle>| s.as_ref().map(|s| of(s.font.as_ref(), "sans-serif"));
        Fonts { list: of(skin.pl_font.as_ref(), &skin.pl.font), text: style(&skin.text), clock: style(&skin.clock) }
    }
}

pub struct Font {
    /// The font itself first, then system fonts picked up for missing
    /// characters.
    faces: Vec<fontdue::Font>,
    /// The system font files already among `faces`.
    files: HashMap<PathBuf, usize>,
    /// Which face draws each character met so far; `None` when none can.
    face_of: HashMap<char, Option<usize>>,
    cache: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

/// Whether `face` draws `c`: maps it to a glyph with an outline. A font
/// fontdue cannot read the outlines of (ttf-parser 0.25 drops them all
/// when `loca` has 65536 entries) maps characters to empty glyphs.
fn draws(face: &fontdue::Font, c: char) -> bool {
    face.lookup_glyph_index(c) != 0 && (c.is_whitespace() || face.metrics(c, 16.0).width > 0)
}

fn parse(data: Vec<u8>, what: &str) -> Option<fontdue::Font> {
    fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).map_err(|e| log::warn!("cannot parse font {what}: {e}")).ok()
}

impl Font {
    fn new(face: Option<fontdue::Font>) -> Font {
        if face.is_none() {
            log::warn!("no usable TrueType font found; text will be blank");
        }
        Font { faces: face.into_iter().collect(), files: HashMap::new(), face_of: HashMap::new(), cache: HashMap::new() }
    }

    pub fn load(family: &str) -> Font {
        Font::new([family, "sans-serif", "DejaVu Sans"].iter().find_map(|f| {
            let path = system::family(f)?;
            parse(std::fs::read(&path).ok()?, &path.display().to_string())
        }))
    }

    /// A font from a font file's bytes, or the system's sans-serif font.
    pub fn from_data(data: &[u8]) -> Font {
        match parse(data.to_vec(), "from the skin") {
            Some(face) => Font::new(Some(face)),
            None => Font::load("sans-serif"),
        }
    }

    /// The face that draws `c`, looking for a system font the first time
    /// the font itself lacks it.
    fn face(&mut self, c: char) -> Option<usize> {
        if let Some(&f) = self.face_of.get(&c) {
            return f;
        }
        let f = self.faces.iter().position(|f| draws(f, c)).or_else(|| self.system_face(c));
        self.face_of.insert(c, f);
        f
    }

    fn system_face(&mut self, c: char) -> Option<usize> {
        if c.is_control() || self.faces.is_empty() {
            return None;
        }
        let candidates = system::for_char(c);
        let found = candidates.into_iter().find_map(|path| self.system_file(path).filter(|&i| draws(&self.faces[i], c)));
        log::debug!("system font for {c:?}: {:?}", found.and_then(|i| self.files.iter().find(|(_, &f)| f == i).map(|(p, _)| p)));
        found
    }

    /// The face of a system font file, read the first time.
    fn system_file(&mut self, path: PathBuf) -> Option<usize> {
        if let Some(&i) = self.files.get(&path) {
            return Some(i);
        }
        let face = parse(std::fs::read(&path).ok()?, &path.display().to_string())?;
        self.faces.push(face);
        self.files.insert(path, self.faces.len() - 1);
        Some(self.faces.len() - 1)
    }

    fn glyph(&mut self, c: char, px: u32) -> Option<&(fontdue::Metrics, Vec<u8>)> {
        // A character no font has still takes the font's own (blank) glyph.
        let i = self.face(c).or((!self.faces.is_empty()).then_some(0))?;
        let face = &self.faces[i];
        if self.cache.len() > 4096 {
            self.cache.clear();
        }
        Some(self.cache.entry((c, px)).or_insert_with(|| face.rasterize(c, px as f32)))
    }

    pub fn ascent(&self, px: u32) -> i32 {
        self.ascent_at(px as f32)
    }

    fn ascent_at(&self, size: f32) -> i32 {
        self.faces.first().and_then(|f| f.horizontal_line_metrics(size)).map(|m| m.ascent.round() as i32).unwrap_or(size.round() as i32)
    }

    pub fn width(&mut self, text: &str, px: u32) -> i32 {
        self.width_at(text, px as f32)
    }

    /// The width of `text` `size` pixels tall, which need not be whole.
    pub fn width_at(&mut self, text: &str, size: f32) -> i32 {
        let (px, step) = raster(size);
        (text.chars().map(|c| self.glyph(c, px).map(|g| g.0.advance_width).unwrap_or(0.0)).sum::<f32>() * step).round() as i32
    }

    /// How far the ink of `text` reaches above the baseline and below it
    /// (negative), drawn `size` pixels tall; `None` for no ink.
    pub fn ink(&mut self, text: &str, size: f32) -> Option<(i32, i32)> {
        let (px, _) = raster(size);
        let boxes: Vec<(i32, i32)> = text
            .chars()
            .filter_map(|c| self.glyph(c, px).map(|g| g.0))
            .filter(|m| m.height > 0)
            .map(|m| (m.ymin + m.height as i32, m.ymin))
            .collect();
        Some((boxes.iter().map(|b| b.0).max()?, boxes.iter().map(|b| b.1).min()?))
    }

    /// Draws `text` with its top at `y`, clipped to `clip_x..clip_x+clip_w`.
    /// Returns the x after the last glyph.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, b: &mut Bitmap, x: i32, y: i32, text: &str, px: u32, color: u32, clip_x: i32, clip_w: i32) -> i32 {
        let baseline = y + self.ascent(px);
        self.draw_at(b, x, baseline, text, px as f32, color, clip_x, clip_w)
    }

    /// Draws `text` `size` pixels tall on `baseline`. A size that is not
    /// whole, as a skin's at x1.5, is drawn with the glyphs of the nearest
    /// whole size, which keeps pixel fonts sharp, spaced as at `size`, so
    /// that the text keeps its length against the rest of the window.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_at(&mut self, b: &mut Bitmap, x: i32, baseline: i32, text: &str, size: f32, color: u32, clip_x: i32, clip_w: i32) -> i32 {
        let (px, step) = raster(size);
        let mut pen = x as f32;
        for c in text.chars() {
            let Some((m, coverage)) = self.glyph(c, px).map(|(m, c)| (*m, c.clone())) else { continue };
            let gx = pen.round() as i32 + m.xmin;
            let gy = baseline - m.height as i32 - m.ymin;
            for row in 0..m.height as i32 {
                for col in 0..m.width as i32 {
                    let a = coverage[(row * m.width as i32 + col) as usize];
                    let (px_x, px_y) = (gx + col, gy + row);
                    if a == 0 || px_x < clip_x || px_x >= clip_x + clip_w {
                        continue;
                    }
                    if let Some(bg) = b.get(px_x, px_y) {
                        b.set(px_x, px_y, mix(bg, color, a as f32 / 255.0));
                    }
                }
            }
            pen += m.advance_width * step;
            if pen as i32 > clip_x + clip_w {
                break;
            }
        }
        pen.round() as i32
    }

    /// Shortens `text` with an ellipsis so it fits in `max_w` pixels.
    pub fn fit(&mut self, text: &str, px: u32, max_w: i32) -> String {
        if self.width(text, px) <= max_w {
            return text.to_string();
        }
        let ell = self.width("…", px);
        let mut out = String::new();
        let mut w = 0;
        for c in text.chars() {
            let cw = self.width(&c.to_string(), px);
            if w + cw + ell > max_w {
                break;
            }
            out.push(c);
            w += cw;
        }
        out.push('…');
        out
    }
}

/// The whole pixel size the glyphs of a text `size` pixels tall are drawn
/// at, and how far apart they go per pixel of their own advance.
fn raster(size: f32) -> (u32, f32) {
    let px = size.round().max(1.0);
    (px as u32, size / px)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn text_at_a_size_between_whole_pixels_keeps_its_length() {
        let mut font = Font::new(parse(CLOCK.to_vec(), "clock"));
        let at_13 = font.width_at("01:23", 13.0);
        // 19.5 is drawn with 20-pixel glyphs, spaced as at 19.5.
        assert_eq!(font.width_at("01:23", 19.5), (at_13 as f32 * 1.5).round() as i32);
        assert!(font.width_at("01:23", 20.0) > font.width_at("01:23", 19.5));
        let (top, bottom) = font.ink("0123456789", 13.0).unwrap();
        assert!(top > 0 && bottom <= 0 && top - bottom <= 13, "{top} {bottom}");
        assert_eq!(font.ink(" ", 13.0), None);
    }

    /// The digits font of the test skins.
    pub(crate) fn clock_font() -> Font {
        Font::new(parse(CLOCK.to_vec(), "clock"))
    }

    const CLOCK: &[u8] = include_bytes!("../../../crates/popamp-skin/tests/fixtures/fonts-shade/fonts/clock.ttf");

    #[test]
    fn a_font_draws_the_characters_it_has_outlines_for() {
        let face = parse(CLOCK.to_vec(), "clock").unwrap();
        assert!(draws(&face, '7') && draws(&face, ' '));
        assert!(!draws(&face, 'A'));
    }

    #[test]
    fn a_font_whose_outlines_cannot_be_read_draws_nothing() {
        // The same font with its `glyf` table hidden: characters still map
        // to glyphs, as in fonts ttf-parser drops the outlines of.
        let mut data = CLOCK.to_vec();
        let at = data.windows(4).position(|w| w == b"glyf").unwrap();
        data[at..at + 4].copy_from_slice(b"xxxx");
        let face = parse(data, "broken").unwrap();
        assert_ne!(face.lookup_glyph_index('7'), 0);
        assert!(!draws(&face, '7'));
    }
}
