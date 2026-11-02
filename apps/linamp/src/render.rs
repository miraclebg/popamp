//! Skin drawing helpers shared by the windows.

use crate::canvas::Canvas;
use linamp_skin::sprites::{main, Rect};
use linamp_skin::textcfg::{glyph_cell, GLYPH_H, GLYPH_W};
use linamp_skin::{Picture, Sheet, Skin};

pub fn sprite(b: &mut Canvas, skin: &Skin, sheet: Sheet, src: Rect, x: i32, y: i32) {
    picture(b, skin, Picture::Sheet(sheet), src, Rect { x, y, w: src.w, h: src.h });
}

/// Draws area `src` of a picture at `(clip.x, clip.y)`, cut to `clip`'s size.
pub fn picture(b: &mut Canvas, skin: &Skin, pic: Picture, src: Rect, clip: Rect) {
    match pic {
        Picture::Sheet(s) => b.blit(skin.sheet(s), src.x, src.y, clip.w.min(src.w), clip.h.min(src.h), clip.x, clip.y),
        Picture::Svg(id) => {
            let at = Rect { x: clip.x, y: clip.y, w: src.w, h: src.h };
            let (w, h) = b.screen_size(at);
            if let Some(r) = skin.svgs.raster(id, src, w, h) {
                b.overlay(&r, at, clip);
            }
        }
    }
}

/// Draws `text` with the skin's `text.bmp` font, clipped to `area`, starting
/// `offset` pixels into the text (for scrolling).
pub fn skin_text(b: &mut Canvas, skin: &Skin, area: Rect, text: &str, offset: i32) {
    let font = skin.sheet(Sheet::Text);
    let chars: Vec<char> = text.chars().collect();
    let first = (offset / GLYPH_W).max(0) as usize;
    let shift = offset.rem_euclid(GLYPH_W);
    let visible = (area.w / GLYPH_W + 2) as usize;
    for i in 0..visible {
        let c = chars.get(first + i).copied().unwrap_or(' ');
        let (cx, cy) = glyph_cell(c);
        let dx = area.x + i as i32 * GLYPH_W - shift;
        for row in 0..GLYPH_H.min(area.h) {
            for col in 0..GLYPH_W {
                let x = dx + col;
                if x < area.x || x >= area.x + area.w {
                    continue;
                }
                if let Some(p) = font.get(cx * GLYPH_W + col, cy * GLYPH_H + row) {
                    b.set(x, area.y + row, p);
                }
            }
        }
    }
}

pub fn skin_text_width(text: &str) -> i32 {
    text.chars().count() as i32 * GLYPH_W
}

/// Draws the big `m:ss` time display; `negative` shows a minus sign.
pub fn time_digits(b: &mut Canvas, skin: &Skin, secs: Option<u64>, negative: bool, ox: i32, oy: i32) {
    let sheet = if skin.has_nums_ex { Sheet::NumsEx } else { Sheet::Numbers };
    let blank = Rect { x: main::NUMBERS_BLANK_X, y: 0, w: main::DIGIT_W, h: main::DIGIT_H };
    let digits: [Option<u64>; 4] = match secs {
        Some(s) => {
            let m = (s / 60).min(99);
            [Some(m / 10), Some(m % 10), Some((s % 60) / 10), Some(s % 10)]
        }
        None => [None; 4],
    };
    for (i, d) in digits.iter().enumerate() {
        let (x, y) = main::AT_DIGITS[i];
        let src = match d {
            Some(d) => Rect { x: *d as i32 * main::DIGIT_W, y: 0, w: main::DIGIT_W, h: main::DIGIT_H },
            None => blank,
        };
        sprite(b, skin, sheet, src, ox + x, oy + y);
    }
    let (mx, my) = main::AT_MINUS;
    if skin.has_nums_ex {
        let src = if negative { main::NUMS_EX_MINUS } else { Rect { x: 90, y: 0, w: 9, h: 13 } };
        sprite(b, skin, sheet, src, ox + mx, oy + my);
    } else if negative {
        sprite(b, skin, Sheet::Numbers, main::NUMBERS_MINUS, ox + mx + 2, oy + my + 6);
    }
}
