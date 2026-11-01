//! Skin drawing helpers shared by the windows.

use linamp_skin::sprites::{main, Rect};
use linamp_skin::textcfg::{glyph_cell, GLYPH_H, GLYPH_W};
use linamp_skin::{Bitmap, Sheet, Skin};

pub fn sprite(b: &mut Bitmap, skin: &Skin, sheet: Sheet, src: Rect, x: i32, y: i32) {
    b.blit(skin.sheet(sheet), src.x, src.y, src.w, src.h, x, y);
}

/// Draws `text` with the skin's `text.bmp` font, clipped to `area`, starting
/// `offset` pixels into the text (for scrolling).
pub fn skin_text(b: &mut Bitmap, skin: &Skin, area: Rect, text: &str, offset: i32) {
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
pub fn time_digits(b: &mut Bitmap, skin: &Skin, secs: Option<u64>, negative: bool, ox: i32, oy: i32) {
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
