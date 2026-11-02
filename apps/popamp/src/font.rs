//! TrueType text for the playlist, menus and vector skins (fontdue).
//! Characters the font lacks come from a system font that has them,
//! found through fontconfig.

use popamp_skin::bitmap::mix;
use popamp_skin::{Bitmap, Skin, TextStyle};
use std::collections::HashMap;
use std::path::PathBuf;

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

/// Finds a font file through `fc-match`: a family name, or a pattern
/// such as `:charset=4e2d` for a font with that character.
fn find_font_file(pattern: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    let path = PathBuf::from(path.trim());
    path.is_file().then_some(path)
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
            let path = find_font_file(f)?;
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
        let f = self.faces.iter().position(|f| f.lookup_glyph_index(c) != 0).or_else(|| self.system_face(c));
        self.face_of.insert(c, f);
        f
    }

    fn system_face(&mut self, c: char) -> Option<usize> {
        if c.is_control() || self.faces.is_empty() {
            return None;
        }
        let path = find_font_file(&format!(":charset={:x}", c as u32))?;
        let i = match self.files.get(&path) {
            Some(&i) => i,
            None => {
                let face = parse(std::fs::read(&path).ok()?, &path.display().to_string())?;
                self.faces.push(face);
                self.files.insert(path, self.faces.len() - 1);
                self.faces.len() - 1
            }
        };
        (self.faces[i].lookup_glyph_index(c) != 0).then_some(i)
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
        self.faces.first().and_then(|f| f.horizontal_line_metrics(px as f32)).map(|m| m.ascent.round() as i32).unwrap_or(px as i32)
    }

    pub fn width(&mut self, text: &str, px: u32) -> i32 {
        text.chars().map(|c| self.glyph(c, px).map(|g| g.0.advance_width).unwrap_or(0.0)).sum::<f32>().round() as i32
    }

    /// Draws `text` with its top at `y`, clipped to `clip_x..clip_x+clip_w`.
    /// Returns the x after the last glyph.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, b: &mut Bitmap, x: i32, y: i32, text: &str, px: u32, color: u32, clip_x: i32, clip_w: i32) -> i32 {
        let baseline = y + self.ascent(px);
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
            pen += m.advance_width;
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
