//! What the default skin draws with: a bitmap, or an SVG drawing of the
//! same picture for its vector version.

use crate::bitmap::Bitmap;
use std::fmt::Write as _;

pub trait Pen {
    /// A `w` x `h` picture filled with `color`.
    fn blank(w: i32, h: i32, color: u32) -> Self;
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32);
    /// Vertical gradient from `top` to `bottom`.
    fn vgradient(&mut self, x: i32, y: i32, w: i32, h: i32, top: u32, bottom: u32);
    fn polygon(&mut self, pts: &[(f32, f32)], c: u32);

    fn set(&mut self, x: i32, y: i32, c: u32) {
        self.fill(x, y, 1, 1, c);
    }

    fn hline(&mut self, x: i32, y: i32, w: i32, c: u32) {
        self.fill(x, y, w, 1, c);
    }

    fn vline(&mut self, x: i32, y: i32, h: i32, c: u32) {
        self.fill(x, y, 1, h, c);
    }

    /// One-pixel bevel: `light` on top/left, `dark` on bottom/right.
    fn bevel(&mut self, x: i32, y: i32, w: i32, h: i32, light: u32, dark: u32) {
        self.hline(x, y, w, light);
        self.vline(x, y, h, light);
        self.hline(x, y + h - 1, w, dark);
        self.vline(x + w - 1, y, h, dark);
    }
}

impl Pen for Bitmap {
    fn blank(w: i32, h: i32, color: u32) -> Self {
        Bitmap::new(w, h, color)
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        Bitmap::fill(self, x, y, w, h, c);
    }

    fn vgradient(&mut self, x: i32, y: i32, w: i32, h: i32, top: u32, bottom: u32) {
        Bitmap::vgradient(self, x, y, w, h, top, bottom);
    }

    fn polygon(&mut self, pts: &[(f32, f32)], c: u32) {
        Bitmap::polygon(self, pts, c);
    }
}

/// An SVG drawing, one shape per call, in skin pixels.
pub struct Svg {
    w: i32,
    h: i32,
    defs: String,
    body: String,
    gradients: usize,
}

fn hex(c: u32) -> String {
    format!("#{:06x}", c & 0xffffff)
}

impl Svg {
    pub fn finish(self) -> String {
        let (w, h) = (self.w, self.h);
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><defs>{}</defs>{}</svg>",
            self.defs, self.body
        )
    }
}

impl Pen for Svg {
    fn blank(w: i32, h: i32, color: u32) -> Self {
        let mut svg = Svg { w, h, defs: String::new(), body: String::new(), gradients: 0 };
        svg.fill(0, 0, w, h, color);
        svg
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        if w > 0 && h > 0 {
            let _ = write!(self.body, "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>", hex(c));
        }
    }

    /// The bitmap colours each row by its place between the first and the
    /// last row; the stops sit on those rows' middles to match.
    fn vgradient(&mut self, x: i32, y: i32, w: i32, h: i32, top: u32, bottom: u32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let id = self.gradients;
        self.gradients += 1;
        let (a, b) = (0.5 / h as f32, (h as f32 - 0.5) / h as f32);
        let _ = write!(
            self.defs,
            "<linearGradient id=\"g{id}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"{a}\" stop-color=\"{}\"/><stop offset=\"{b}\" stop-color=\"{}\"/></linearGradient>",
            hex(top),
            hex(bottom)
        );
        let _ = write!(self.body, "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"url(#g{id})\"/>");
    }

    fn polygon(&mut self, pts: &[(f32, f32)], c: u32) {
        let points: Vec<String> = pts.iter().map(|(x, y)| format!("{x},{y}")).collect();
        let _ = write!(self.body, "<polygon points=\"{}\" fill=\"{}\"/>", points.join(" "), hex(c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprites::Rect;
    use crate::svg::Svgs;

    #[test]
    fn svg_drawings_match_the_bitmap_on_whole_pixels() {
        fn draw<P: Pen>() -> P {
            let mut p = P::blank(6, 4, 0x112233);
            p.vgradient(0, 0, 6, 3, 0x000000, 0x0000ff);
            p.bevel(1, 1, 4, 3, 0xffffff, 0x808080);
            p
        }
        let bitmap: Bitmap = draw();
        let mut svgs = Svgs::default();
        let id = svgs.add(draw::<Svg>().finish().as_bytes()).unwrap();
        let r = svgs.raster(id, Rect { x: 0, y: 0, w: 6, h: 4 }, 6, 4).unwrap();
        let rgb: Vec<u32> = r.px.iter().map(|p| p & 0xffffff).collect();
        // Gradient rows may round one step apart.
        let close = |a: u32, b: u32| (0..3).all(|i| (((a >> (8 * i)) & 0xff) as i32 - ((b >> (8 * i)) & 0xff) as i32).abs() <= 1);
        assert!(rgb.iter().zip(&bitmap.px).all(|(&a, &b)| close(a, b)), "{rgb:x?}\n{:x?}", bitmap.px);
    }
}
