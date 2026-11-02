//! A window's picture in skin pixels. Drawing calls take skin coordinates;
//! the canvas puts them on a bitmap `scale` times larger, so a vector skin
//! can be drawn straight at screen size instead of being blown up later.

use crate::font::Font;
use crate::scale::Scale;
use linamp_skin::{Bitmap, Rect, Rgba};

pub struct Canvas<'a> {
    pub bitmap: &'a mut Bitmap,
    pub scale: Scale,
}

impl<'a> Canvas<'a> {
    pub fn new(bitmap: &'a mut Bitmap, scale: Scale) -> Self {
        Canvas { bitmap, scale }
    }

    /// The screen rectangle that skin rectangle `x, y, w, h` covers; edges
    /// round the same way everywhere, so neighbours meet without gaps.
    fn screen(&self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
        let s = self.scale;
        let (x0, y0) = (s.px(x), s.px(y));
        (x0, y0, s.px(x + w) - x0, s.px(y + h) - y0)
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let (x, y, w, h) = self.screen(x, y, w, h);
        self.bitmap.fill(x, y, w, h, c);
    }

    pub fn set(&mut self, x: i32, y: i32, c: u32) {
        self.fill(x, y, 1, 1, c);
    }

    /// Copies a rectangle of `src` to `(dx, dy)`, each skin pixel blown up
    /// to the screen pixels it covers.
    #[allow(clippy::too_many_arguments)]
    pub fn blit(&mut self, src: &Bitmap, sx: i32, sy: i32, w: i32, h: i32, dx: i32, dy: i32) {
        if self.scale.is_one() {
            return self.bitmap.blit(src, sx, sy, w, h, dx, dy);
        }
        let s = self.scale;
        let (x0, y0, pw, ph) = self.screen(dx, dy, w, h);
        for y in y0..y0 + ph {
            let row = sy + s.skin(y) - dy;
            for x in x0..x0 + pw {
                if let Some(p) = src.get(sx + s.skin(x) - dx, row) {
                    self.bitmap.set(x, y, p);
                }
            }
        }
    }

    /// Lays `src` over the screen rectangle that skin rectangle `at`
    /// covers (`src` is that size), clipped to skin rectangle `clip`.
    pub fn overlay(&mut self, src: &Rgba, at: Rect, clip: Rect) {
        let (x, y, _, _) = self.screen(at.x, at.y, at.w, at.h);
        self.bitmap.overlay(src, x, y, self.screen(clip.x, clip.y, clip.w, clip.h));
    }

    /// The screen size of skin rectangle `r`.
    pub fn screen_size(&self, r: Rect) -> (i32, i32) {
        let (_, _, w, h) = self.screen(r.x, r.y, r.w, r.h);
        (w, h)
    }

    /// Draws `text` in a TrueType font `size` skin pixels tall with its top
    /// at `(x, y)`, clipped to columns `clip_x..clip_x + clip_w`.
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, font: &mut Font, x: i32, y: i32, text: &str, size: u32, color: u32, clip: Rect) {
        let s = self.scale;
        let (cx, _, cw, _) = self.screen(clip.x, clip.y, clip.w, clip.h);
        font.draw(self.bitmap, s.px(x), s.px(y), text, s.px(size as i32) as u32, color, cx, cw);
    }

    /// The width of `text` in skin pixels, rounded up.
    pub fn text_width(&self, font: &mut Font, text: &str, size: u32) -> i32 {
        let s = self.scale;
        s.skin_up(font.width(text, s.px(size as i32) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linamp_core::config::Scaling;

    #[test]
    fn skin_pixels_cover_whole_screen_pixels_without_gaps() {
        let mut b = Bitmap::new(6, 1, 0);
        let mut c = Canvas::new(&mut b, Scale::new(150, Scaling::Sharp));
        c.set(0, 0, 1);
        c.set(1, 0, 2);
        c.fill(2, 0, 2, 1, 3);
        assert_eq!(b.px, [1, 1, 2, 3, 3, 3]);
    }

    #[test]
    fn blit_blows_pixels_up() {
        let mut src = Bitmap::new(2, 1, 0);
        src.px.copy_from_slice(&[7, 9]);
        let mut b = Bitmap::new(5, 2, 0);
        Canvas::new(&mut b, Scale::new(200, Scaling::Sharp)).blit(&src, 0, 0, 2, 1, 0, 0);
        assert_eq!(b.px, [7, 7, 9, 9, 0, 7, 7, 9, 9, 0]);
    }
}
