//! Drawing for the menus, tooltips and Popamp's own dialogs at the size of
//! the skin's windows. Callers give coordinates in dialog pixels, as at ×1;
//! the pen draws them `pct` / 100 times as large. Text is set at that size
//! rather than stretched, so it stays sharp at ×2 or ×3.

use crate::font::Font;
use crate::menu::FONT_PX;
use crate::widgets::{fit, Rect};
use popamp_skin::Bitmap;

/// The screen length of `n` dialog pixels at `pct` percent.
pub fn screen(pct: i32, n: i32) -> i32 {
    (n * pct + 50).div_euclid(100)
}

/// The dialog pixel at screen offset `p`.
pub fn dialog(pct: i32, p: i32) -> i32 {
    (p * 100).div_euclid(pct)
}

/// The size in screen pixels that `n` dialog pixels reach at the largest
/// window size: how large a picture shown in `n` pixels is worth keeping.
pub fn largest(n: i32) -> i32 {
    screen(popamp_core::config::ZOOMS[popamp_core::config::ZOOMS.len() - 1], n)
}

/// The width of `text` in dialog pixels.
pub fn text_width(font: &mut Font, text: &str, pct: i32) -> i32 {
    (font.width(text, text_px(pct)) * 100 + pct - 1).div_euclid(pct)
}

fn text_px(pct: i32) -> u32 {
    screen(pct, FONT_PX as i32).max(1) as u32
}

pub struct Pen<'a> {
    b: &'a mut Bitmap,
    font: &'a mut Font,
    pct: i32,
}

impl<'a> Pen<'a> {
    pub fn new(b: &'a mut Bitmap, font: &'a mut Font, pct: i32) -> Pen<'a> {
        Pen { b, font, pct: pct.max(100) }
    }

    fn at(&self, n: i32) -> i32 {
        screen(self.pct, n)
    }

    /// Where `x..x + w` starts on the screen, and how long it is there.
    fn span(&self, x: i32, w: i32) -> (i32, i32) {
        let x0 = self.at(x);
        (x0, self.at(x + w) - x0)
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let ((sx, sw), (sy, sh)) = (self.span(x, w), self.span(y, h));
        self.b.fill(sx, sy, sw, sh, c);
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: u32) {
        self.fill(x, y, w, 1, c);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, c: u32) {
        self.fill(x, y, 1, h, c);
    }

    pub fn set(&mut self, x: i32, y: i32, c: u32) {
        self.fill(x, y, 1, 1, c);
    }

    /// A one-pixel frame: `light` on top and left, `dark` on bottom and right.
    pub fn bevel(&mut self, x: i32, y: i32, w: i32, h: i32, light: u32, dark: u32) {
        self.hline(x, y, w, light);
        self.vline(x, y, h, light);
        self.hline(x, y + h - 1, w, dark);
        self.vline(x + w - 1, y, h, dark);
    }

    pub fn width(&mut self, text: &str) -> i32 {
        text_width(self.font, text, self.pct)
    }

    /// `text`, shortened with an ellipsis to fit in `max_w`.
    pub fn fit(&mut self, text: &str, max_w: i32) -> String {
        let max = self.at(max_w);
        self.font.fit(text, text_px(self.pct), max)
    }

    /// Draws `text` with its top at `y`, clipped to `clip_x..clip_x + clip_w`.
    pub fn text(&mut self, x: i32, y: i32, text: &str, color: u32, clip_x: i32, clip_w: i32) {
        let (cx, cw) = self.span(clip_x, clip_w);
        let (sx, sy) = (self.at(x), self.at(y));
        self.font.draw(self.b, sx, sy, text, text_px(self.pct), color, cx, cw);
    }

    /// Draws `img` centred in `r`, a pixel of it to a dialog pixel, or
    /// shrunk to fit when it is larger than `r` on the screen.
    pub fn image(&mut self, img: &Bitmap, r: Rect) {
        let ((rx, rw), (ry, rh)) = (self.span(r.x, r.w), self.span(r.y, r.h));
        let (nw, nh) = (self.at(img.w), self.at(img.h));
        let shown = if nw > rw || nh > rh {
            fit(img, rw, rh)
        } else if (nw, nh) != (img.w, img.h) {
            nearest(img, nw, nh)
        } else {
            img.clone()
        };
        self.b.blit(&shown, 0, 0, shown.w, shown.h, rx + (rw - shown.w) / 2, ry + (rh - shown.h) / 2);
    }
}

/// `img` stretched to `w`×`h`, each pixel copied from the one it covers.
fn nearest(img: &Bitmap, w: i32, h: i32) -> Bitmap {
    let mut out = Bitmap::new(w, h, 0);
    for y in 0..h {
        let sy = y * img.h / h;
        for x in 0..w {
            out.px[(y * w + x) as usize] = img.px[(sy * img.w + x * img.w / w) as usize];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use popamp_skin::bitmap::rgb;

    const WHITE: u32 = rgb(255, 255, 255);

    const CLOCK: &[u8] = include_bytes!("../../../crates/popamp-skin/tests/fixtures/fonts-shade/fonts/clock.ttf");

    fn font() -> Font {
        Font::from_data(CLOCK)
    }

    /// The rows of `b` with any pixel other than black.
    fn inked_rows(b: &Bitmap) -> i32 {
        (0..b.h).filter(|&y| (0..b.w).any(|x| b.get(x, y) != Some(0))).count() as i32
    }

    #[test]
    fn shapes_grow_with_the_size() {
        let mut b = Bitmap::new(40, 40, 0);
        let mut f = font();
        let mut pen = Pen::new(&mut b, &mut f, 200);
        pen.fill(2, 3, 4, 5, WHITE);
        assert_eq!(b.get(4, 6), Some(WHITE));
        assert_eq!(b.get(11, 15), Some(WHITE));
        assert_eq!(b.get(12, 16), Some(0));
        assert_eq!(b.get(3, 6), Some(0));
    }

    #[test]
    fn text_is_set_larger_not_stretched() {
        let mut f = font();
        let mut small = Bitmap::new(200, 60, 0);
        Pen::new(&mut small, &mut f, 100).text(0, 0, "0:47", WHITE, 0, 200);
        let mut big = Bitmap::new(200, 60, 0);
        Pen::new(&mut big, &mut f, 300).text(0, 0, "0:47", WHITE, 0, 200);
        let (s, l) = (inked_rows(&small), inked_rows(&big));
        assert!(s > 0 && l >= 3 * s - 3 && l <= 3 * s + 3, "{s} rows at ×1, {l} at ×3");
        let w100 = text_width(&mut f, "0:47", 100);
        let w300 = text_width(&mut f, "0:47", 300);
        assert!((w100 - w300).abs() <= 2, "the same width in dialog pixels: {w100} and {w300}");
    }

    #[test]
    fn screen_and_dialog_pixels_convert_both_ways() {
        assert_eq!(screen(150, 10), 15);
        assert_eq!(dialog(150, 15), 10);
        assert_eq!(dialog(300, 29), 9);
    }

    #[test]
    fn small_pictures_grow_and_large_ones_shrink_to_the_frame() {
        let mut f = font();
        let img = Bitmap::new(4, 4, WHITE);
        let mut b = Bitmap::new(30, 30, 0);
        Pen::new(&mut b, &mut f, 300).image(&img, Rect::new(0, 0, 10, 10));
        assert_eq!(inked_rows(&b), 12, "a 4-pixel picture is 12 screen pixels at ×3");
        let big = Bitmap::new(100, 50, WHITE);
        let mut b = Bitmap::new(30, 30, 0);
        Pen::new(&mut b, &mut f, 300).image(&big, Rect::new(0, 0, 10, 10));
        assert_eq!(inked_rows(&b), 15, "shrunk to the 30 screen pixels of the frame");
    }
}
