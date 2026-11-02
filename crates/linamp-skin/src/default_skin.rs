//! Linamp's own default skin, drawn procedurally at the classic sprite
//! geometry. No artwork from any other product is used.

use crate::bitmap::{mix, rgb, Bitmap};
use crate::pen::Pen;
use crate::sprites::{eq, main, pl, r, Rect, Sheet};
use crate::textcfg::{PlColors, GLYPH_H, GLYPH_W};

pub const PANEL: u32 = rgb(0x26, 0x2b, 0x38);
pub const PANEL_HI: u32 = rgb(0x3b, 0x43, 0x58);
pub const PANEL_LO: u32 = rgb(0x12, 0x15, 0x1c);
pub const LCD: u32 = rgb(0x06, 0x09, 0x0d);
pub const LCD_ON: u32 = rgb(0x52, 0xe3, 0xa0);
pub const LCD_DIM: u32 = rgb(0x15, 0x36, 0x29);
pub const ACCENT: u32 = rgb(0xf2, 0xa9, 0x3b);
pub const ACCENT_LO: u32 = rgb(0x9c, 0x62, 0x14);
pub const ICON: u32 = rgb(0xe6, 0xea, 0xf2);
pub const BTN_TOP: u32 = rgb(0x5d, 0x67, 0x80);
pub const BTN_BOT: u32 = rgb(0x39, 0x40, 0x52);
pub const TITLE_TOP: u32 = rgb(0x4d, 0x62, 0x93);
pub const TITLE_BOT: u32 = rgb(0x2a, 0x33, 0x4b);
pub const TITLE_IN_TOP: u32 = rgb(0x3a, 0x3e, 0x48);
pub const TITLE_IN_BOT: u32 = rgb(0x25, 0x28, 0x2f);

/// 4x5 glyphs; rows of '#' and '.'.
fn glyph(c: char) -> Option<[&'static str; 5]> {
    Some(match c {
        'a' => [".##.", "#..#", "####", "#..#", "#..#"],
        'b' => ["###.", "#..#", "###.", "#..#", "###."],
        'c' => [".###", "#...", "#...", "#...", ".###"],
        'd' => ["###.", "#..#", "#..#", "#..#", "###."],
        'e' => ["####", "#...", "###.", "#...", "####"],
        'f' => ["####", "#...", "###.", "#...", "#..."],
        'g' => [".###", "#...", "#.##", "#..#", ".###"],
        'h' => ["#..#", "#..#", "####", "#..#", "#..#"],
        'i' => ["###.", ".#..", ".#..", ".#..", "###."],
        'j' => ["..##", "...#", "...#", "#..#", ".##."],
        'k' => ["#..#", "#.#.", "##..", "#.#.", "#..#"],
        'l' => ["#...", "#...", "#...", "#...", "####"],
        'm' => ["#..#", "####", "####", "#..#", "#..#"],
        'n' => ["#..#", "##.#", "#.##", "#..#", "#..#"],
        'o' => [".##.", "#..#", "#..#", "#..#", ".##."],
        'p' => ["###.", "#..#", "###.", "#...", "#..."],
        'q' => [".##.", "#..#", "#..#", "#.##", ".###"],
        'r' => ["###.", "#..#", "###.", "#.#.", "#..#"],
        's' => [".###", "#...", ".##.", "...#", "###."],
        't' => ["###.", ".#..", ".#..", ".#..", ".#.."],
        'u' => ["#..#", "#..#", "#..#", "#..#", ".##."],
        'v' => ["#..#", "#..#", "#..#", ".##.", ".##."],
        'w' => ["#..#", "#..#", "####", "####", "#..#"],
        'x' => ["#..#", "#..#", ".##.", "#..#", "#..#"],
        'y' => ["#..#", "#..#", ".##.", ".#..", ".#.."],
        'z' => ["####", "...#", ".##.", "#...", "####"],
        '0' => [".##.", "#.##", "##.#", "#..#", ".##."],
        '1' => [".#..", "##..", ".#..", ".#..", "###."],
        '2' => ["###.", "...#", ".##.", "#...", "####"],
        '3' => ["###.", "...#", ".##.", "...#", "###."],
        '4' => ["#..#", "#..#", "####", "...#", "...#"],
        '5' => ["####", "#...", "###.", "...#", "###."],
        '6' => [".##.", "#...", "###.", "#..#", ".##."],
        '7' => ["####", "...#", "..#.", ".#..", ".#.."],
        '8' => [".##.", "#..#", ".##.", "#..#", ".##."],
        '9' => [".##.", "#..#", ".###", "...#", ".##."],
        '"' => ["#.#.", "#.#.", "....", "....", "...."],
        '@' => [".##.", "#.##", "#.##", "#...", ".###"],
        '…' => ["....", "....", "....", "....", "#.#."],
        '.' => ["....", "....", "....", "....", ".#.."],
        ':' => ["....", ".#..", "....", ".#..", "...."],
        '(' => ["..#.", ".#..", ".#..", ".#..", "..#."],
        ')' => [".#..", "..#.", "..#.", "..#.", ".#.."],
        '-' => ["....", "....", "###.", "....", "...."],
        '\'' => [".#..", ".#..", "....", "....", "...."],
        '!' => [".#..", ".#..", ".#..", "....", ".#.."],
        '_' => ["....", "....", "....", "....", "####"],
        '+' => ["....", ".#..", "###.", ".#..", "...."],
        '\\' => ["#...", "#...", ".#..", "..#.", "..#."],
        '/' => ["..#.", "..#.", ".#..", "#...", "#..."],
        '[' => [".##.", ".#..", ".#..", ".#..", ".##."],
        ']' => [".##.", "..#.", "..#.", "..#.", ".##."],
        '^' => [".#..", "#.#.", "....", "....", "...."],
        '&' => [".#..", "#.#.", ".#..", "#.#.", ".#.#"],
        '%' => ["#..#", "..#.", ".#..", "#...", "#..#"],
        ',' => ["....", "....", "....", ".#..", "#..."],
        '=' => ["....", "###.", "....", "###.", "...."],
        '$' => [".###", "##..", ".##.", "..##", "###."],
        '#' => [".#.#", "####", ".#.#", "####", ".#.#"],
        'å' => [".##.", ".##.", "#..#", "####", "#..#"],
        'ö' => ["#..#", ".##.", "#..#", "#..#", ".##."],
        'ä' => ["#..#", ".##.", "#..#", "####", "#..#"],
        '?' => ["###.", "...#", ".##.", "....", ".#.."],
        '*' => ["....", "#.#.", ".#..", "#.#.", "...."],
        _ => return None,
    })
}

/// Draws text with the built-in 4x5 font (5 px advance).
pub fn draw_text(b: &mut impl Pen, x: i32, y: i32, text: &str, color: u32) {
    for (i, c) in text.chars().enumerate() {
        let c = c.to_lowercase().next().unwrap_or(c);
        if let Some(rows) = glyph(c) {
            for (gy, row) in rows.iter().enumerate() {
                for (gx, px) in row.bytes().enumerate() {
                    if px == b'#' {
                        b.set(x + i as i32 * GLYPH_W + gx as i32, y + gy as i32, color);
                    }
                }
            }
        }
    }
}

fn text_width(text: &str) -> i32 {
    text.chars().count() as i32 * GLYPH_W - 1
}

fn draw_text_centered(b: &mut impl Pen, rect: Rect, text: &str, color: u32) {
    let x = rect.x + (rect.w - text_width(text)) / 2;
    let y = rect.y + (rect.h - 5) / 2;
    draw_text(b, x, y, text, color);
}

fn lcd_box(b: &mut impl Pen, x: i32, y: i32, w: i32, h: i32) {
    b.fill(x, y, w, h, LCD);
    b.bevel(x - 1, y - 1, w + 2, h + 2, PANEL_LO, PANEL_HI);
}

fn panel(b: &mut impl Pen, x: i32, y: i32, w: i32, h: i32) {
    b.vgradient(x, y, w, h, PANEL_HI, PANEL);
    b.bevel(x, y, w, h, mix(PANEL_HI, ICON, 0.25), PANEL_LO);
}

fn button(b: &mut impl Pen, rect: Rect, pressed: bool) {
    let (top, bot) = if pressed { (mix(BTN_BOT, PANEL_LO, 0.4), BTN_BOT) } else { (BTN_TOP, BTN_BOT) };
    b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
    let (light, dark) = if pressed { (PANEL_LO, BTN_TOP) } else { (mix(BTN_TOP, ICON, 0.35), PANEL_LO) };
    b.bevel(rect.x, rect.y, rect.w, rect.h, light, dark);
}

fn led(b: &mut impl Pen, x: i32, y: i32, on: bool) {
    b.fill(x, y, 3, 3, if on { ACCENT } else { PANEL_LO });
}

/// Title bar with grip lines and a centred label.
fn title_bar(b: &mut impl Pen, rect: Rect, label: &str, active: bool, buttons: bool) {
    let (top, bot) = if active { (TITLE_TOP, TITLE_BOT) } else { (TITLE_IN_TOP, TITLE_IN_BOT) };
    b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
    b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.3), PANEL_LO);
    let text_color = if active { ICON } else { mix(ICON, PANEL, 0.5) };
    let grip = if active { mix(top, ICON, 0.45) } else { mix(top, ICON, 0.15) };
    let tw = text_width(label);
    let cx = rect.x + rect.w / 2;
    let right_end = rect.x + rect.w - if buttons { 34 } else { 6 };
    for gy in [rect.y + rect.h / 2 - 2, rect.y + rect.h / 2 + 1] {
        b.hline(rect.x + 20, gy, cx - tw / 2 - 6 - (rect.x + 20), grip);
        b.hline(cx + tw / 2 + 6, gy, right_end - (cx + tw / 2 + 6), grip);
    }
    draw_text(b, cx - tw / 2, rect.y + (rect.h - 5) / 2, label, text_color);
}

fn small_button(b: &mut impl Pen, x: i32, y: i32, pressed: bool, icon: &str) {
    button(b, r(x, y, 9, 9), pressed);
    let c = ICON;
    match icon {
        "menu" => {
            for i in 0..3 {
                b.hline(x + 2, y + 2 + i * 2, 5, c);
            }
        }
        "min" => b.hline(x + 2, y + 6, 5, c),
        "shade" => {
            b.hline(x + 2, y + 2, 5, c);
            b.hline(x + 2, y + 3, 5, c);
        }
        "unshade" => {
            b.hline(x + 2, y + 5, 5, c);
            b.hline(x + 2, y + 6, 5, c);
        }
        _ => {
            for i in 0..5 {
                b.set(x + 2 + i, y + 2 + i, c);
                b.set(x + 6 - i, y + 2 + i, c);
            }
        }
    }
}

fn gen_text<P: Pen>() -> P {
    let (w, h) = Sheet::Text.size();
    let mut b = P::blank(w, h, LCD);
    let rows = ["abcdefghijklmnopqrstuvwxyz\"@", "0123456789….:()-'!_+\\/[]^&%,=$#", "åöä?*"];
    for (ry, row) in rows.iter().enumerate() {
        for (cx, c) in row.chars().enumerate() {
            draw_text(&mut b, cx as i32 * GLYPH_W, ry as i32 * GLYPH_H, &c.to_string(), LCD_ON);
        }
    }
    b
}

/// Seven-segment digit in a 9x13 cell.
fn seven_seg(b: &mut impl Pen, x: i32, y: i32, digit: Option<u8>) {
    // Segments: a b c d e f g
    const MAP: [[bool; 7]; 10] = [
        [true, true, true, true, true, true, false],
        [false, true, true, false, false, false, false],
        [true, true, false, true, true, false, true],
        [true, true, true, true, false, false, true],
        [false, true, true, false, false, true, true],
        [true, false, true, true, false, true, true],
        [true, false, true, true, true, true, true],
        [true, true, true, false, false, false, false],
        [true, true, true, true, true, true, true],
        [true, true, true, true, false, true, true],
    ];
    let segs = digit.map(|d| MAP[d as usize]).unwrap_or([false; 7]);
    let col = |on: bool| if on { LCD_ON } else { LCD_DIM };
    b.fill(x + 2, y, 5, 2, col(segs[0])); // a
    b.fill(x + 7, y + 2, 2, 3, col(segs[1])); // b
    b.fill(x + 7, y + 7, 2, 4, col(segs[2])); // c
    b.fill(x + 2, y + 11, 5, 2, col(segs[3])); // d
    b.fill(x, y + 7, 2, 4, col(segs[4])); // e
    b.fill(x, y + 2, 2, 3, col(segs[5])); // f
    b.fill(x + 2, y + 5, 5, 2, col(segs[6])); // g (row 6 doubles as the minus sign)
}

fn gen_numbers<P: Pen>(ex: bool) -> P {
    let sheet = if ex { Sheet::NumsEx } else { Sheet::Numbers };
    let (w, h) = sheet.size();
    let mut b = P::blank(w, h, LCD);
    for d in 0..10u8 {
        seven_seg(&mut b, d as i32 * 9, 0, Some(d));
    }
    seven_seg(&mut b, 90, 0, None);
    if ex {
        seven_seg(&mut b, 99, 0, None);
        b.fill(99 + 2, 5, 5, 2, LCD_ON);
    }
    b
}

fn gen_titlebar<P: Pen>() -> P {
    let (w, h) = Sheet::TitleBar.size();
    let mut b = P::blank(w, h, PANEL);
    small_button(&mut b, 0, 0, false, "menu");
    small_button(&mut b, 0, 9, true, "menu");
    small_button(&mut b, 9, 0, false, "min");
    small_button(&mut b, 9, 9, true, "min");
    small_button(&mut b, 18, 0, false, "close");
    small_button(&mut b, 18, 9, true, "close");
    small_button(&mut b, 0, 18, false, "shade");
    small_button(&mut b, 9, 18, true, "shade");
    small_button(&mut b, 0, 27, false, "unshade");
    small_button(&mut b, 9, 27, true, "unshade");
    for (rect, active) in
        [(main::TITLE_ACTIVE, true), (main::TITLE_INACTIVE, false), (r(27, 57, 275, 14), true), (r(27, 72, 275, 14), false)]
    {
        title_bar(&mut b, rect, "LINAMP", active, true);
    }
    for (rect, active) in [(main::SHADE_ACTIVE, true), (main::SHADE_INACTIVE, false)] {
        let (top, bot) = if active { (TITLE_TOP, TITLE_BOT) } else { (TITLE_IN_TOP, TITLE_IN_BOT) };
        b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
        b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.3), PANEL_LO);
        draw_text(&mut b, rect.x + 20, rect.y + 5, "LINAMP", if active { ICON } else { mix(ICON, PANEL, 0.5) });
        // Mini time display and position groove.
        b.fill(rect.x + 125, rect.y + 3, 30, 8, LCD);
        b.fill(rect.x + 165, rect.y + 3, 56, 8, mix(top, PANEL_LO, 0.4));
        b.fill(rect.x + 226, rect.y + 4, 17, 7, LCD);
    }
    // Shade-mode position bar pieces.
    b.fill(0, 36, 17, 7, LCD);
    b.hline(0, 39, 17, LCD_DIM);
    for x in [17, 20, 23] {
        b.fill(x, 36, 3, 7, ACCENT);
    }
    // Clutter bar: normal and disabled backgrounds, then highlighted letters.
    for (x, color) in [(304, mix(LCD_ON, PANEL, 0.6)), (312, PANEL_HI)] {
        b.fill(x, 0, 8, 43, LCD);
        for (i, ch) in "oaidv".chars().enumerate() {
            draw_text(&mut b, x + 2, main::CLUTTER_OFFSETS[i] + 1, &ch.to_string(), color);
        }
    }
    for (i, rect) in main::CLUTTER_SELECTED.iter().enumerate() {
        b.fill(rect.x, rect.y, rect.w, rect.h, LCD_DIM);
        draw_text(&mut b, rect.x + 2, rect.y + 1, &"oaidv"[i..i + 1], LCD_ON);
    }
    b
}

fn transport_icon(b: &mut impl Pen, rect: Rect, kind: usize) {
    let cx = rect.x as f32 + rect.w as f32 / 2.0;
    let cy = rect.y as f32 + rect.h as f32 / 2.0;
    let c = ICON;
    match kind {
        0 => {
            // previous: bar + triangle pointing left
            b.fill((cx - 5.0) as i32, (cy - 4.0) as i32, 2, 8, c);
            b.polygon(&[(cx + 4.0, cy - 4.0), (cx + 4.0, cy + 4.0), (cx - 3.0, cy)], c);
        }
        1 => b.polygon(&[(cx - 3.0, cy - 5.0), (cx - 3.0, cy + 5.0), (cx + 5.0, cy)], c),
        2 => {
            b.fill((cx - 4.0) as i32, (cy - 4.0) as i32, 3, 8, c);
            b.fill((cx + 1.0) as i32, (cy - 4.0) as i32, 3, 8, c);
        }
        3 => b.fill((cx - 4.0) as i32, (cy - 4.0) as i32, 8, 8, c),
        4 => {
            b.polygon(&[(cx - 4.0, cy - 4.0), (cx - 4.0, cy + 4.0), (cx + 3.0, cy)], c);
            b.fill((cx + 3.0) as i32, (cy - 4.0) as i32, 2, 8, c);
        }
        _ => {
            b.polygon(&[(cx - 5.0, cy + 1.0), (cx + 5.0, cy + 1.0), (cx, cy - 5.0)], c);
            b.fill((cx - 5.0) as i32, (cy + 3.0) as i32, 10, 2, c);
        }
    }
}

fn gen_cbuttons<P: Pen>() -> P {
    let (w, h) = Sheet::CButtons.size();
    let mut b = P::blank(w, h, PANEL);
    let buttons = [main::PREV, main::PLAY, main::PAUSE, main::STOP, main::NEXT, main::EJECT];
    for (i, btn) in buttons.iter().enumerate() {
        for (rect, pressed) in [(btn.normal, false), (btn.pressed, true)] {
            button(&mut b, rect, pressed);
            transport_icon(&mut b, rect, i);
        }
    }
    b
}

fn gen_shufrep<P: Pen>() -> P {
    let (w, h) = Sheet::ShufRep.size();
    let mut b = P::blank(w, h, PANEL);
    for (toggle, label) in [(main::REPEAT, "rep"), (main::SHUFFLE, "shuffle"), (main::EQ, "eq"), (main::PL, "pl")] {
        for on in [false, true] {
            for pressed in [false, true] {
                let rect = toggle.pick(on, pressed);
                button(&mut b, rect, pressed);
                led(&mut b, rect.x + 3, rect.y + rect.h / 2 - 1, on);
                let tx = rect.x + 8 + (rect.w - 8 - text_width(label)) / 2;
                draw_text(&mut b, tx, rect.y + (rect.h - 5) / 2, label, if on { ICON } else { mix(ICON, BTN_BOT, 0.35) });
            }
        }
    }
    b
}

fn gen_playpaus<P: Pen>() -> P {
    let (w, h) = Sheet::PlayPaus.size();
    let mut b = P::blank(w, h, LCD);
    b.polygon(&[(2.0, 1.0), (2.0, 8.0), (8.0, 4.5)], LCD_ON);
    b.fill(10, 1, 2, 7, ACCENT);
    b.fill(14, 1, 2, 7, ACCENT);
    b.fill(20, 2, 5, 5, mix(LCD_ON, LCD, 0.5));
    b.fill(36, 0, 3, 9, LCD);
    b.fill(39, 0, 3, 9, LCD);
    b.fill(40, 1, 1, 7, LCD_ON);
    b
}

fn gen_monoster<P: Pen>() -> P {
    let (w, h) = Sheet::MonoSter.size();
    let mut b = P::blank(w, h, LCD);
    for (rect, label, on) in [
        (main::STEREO_ON, "stereo", true),
        (main::STEREO_OFF, "stereo", false),
        (main::MONO_ON, "mono", true),
        (main::MONO_OFF, "mono", false),
    ] {
        b.fill(rect.x, rect.y, rect.w, rect.h, LCD);
        draw_text_centered(&mut b, rect, label, if on { LCD_ON } else { LCD_DIM });
    }
    b
}

fn gen_posbar<P: Pen>() -> P {
    let (w, h) = Sheet::PosBar.size();
    let mut b = P::blank(w, h, PANEL);
    b.fill(0, 0, 248, 10, PANEL_LO);
    b.bevel(0, 0, 248, 10, PANEL_LO, PANEL_HI);
    b.hline(2, 5, 244, LCD_DIM);
    for (rect, top, bot) in [(main::POS_THUMB.normal, ACCENT, ACCENT_LO), (main::POS_THUMB.pressed, mix(ACCENT, ICON, 0.4), ACCENT)] {
        b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
        b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.4), ACCENT_LO);
        for i in 0..3 {
            b.vline(rect.x + 11 + i * 3, rect.y + 2, 6, ACCENT_LO);
        }
    }
    b
}

/// Volume/balance frames: a filled meter whose colour runs green → amber → red.
fn gen_slider<P: Pen>(balance: bool) -> P {
    let (w, h) = Sheet::Volume.size();
    let mut b = P::blank(w, h, PANEL);
    let (x0, fw) = if balance { (main::BALANCE_SRC_X, 38) } else { (0, 68) };
    for k in 0..main::SLIDER_FRAMES {
        let y = k * main::SLIDER_FRAME_STEP;
        let t = k as f32 / 27.0;
        b.fill(x0, y, fw, 13, PANEL_LO);
        b.bevel(x0, y, fw, 13, PANEL_LO, PANEL_HI);
        let color = if t < 0.5 { mix(LCD_ON, ACCENT, t * 2.0) } else { mix(ACCENT, rgb(0xe8, 0x4a, 0x3a), (t - 0.5) * 2.0) };
        if balance {
            let half = ((fw - 4) / 2) as f32;
            let len = (half * t).round() as i32;
            let cx = x0 + fw / 2;
            b.fill(cx - len, y + 5, len * 2 + 1, 3, color);
            b.vline(cx, y + 3, 7, mix(ICON, PANEL, 0.4));
        } else {
            // Rising wedge.
            let len = ((fw - 4) as f32 * t).round() as i32;
            for i in 0..len {
                let hh = 1 + (i * 7) / (fw - 4);
                b.vline(x0 + 2 + i, y + 10 - hh, hh, color);
            }
        }
    }
    for (rect, top) in [(main::SLIDER_THUMB.normal, ACCENT), (main::SLIDER_THUMB.pressed, mix(ACCENT, ICON, 0.4))] {
        b.vgradient(rect.x, rect.y, rect.w, rect.h, top, ACCENT_LO);
        b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.4), ACCENT_LO);
        b.vline(rect.x + 6, rect.y + 2, 7, ACCENT_LO);
    }
    b
}

fn gen_main<P: Pen>() -> P {
    let (w, h) = main::SIZE;
    let mut b = P::blank(w, h, PANEL);
    panel(&mut b, 0, 0, w, h);
    // Display block: clutter bar, status, time, vis.
    lcd_box(&mut b, 9, 21, 94, 40);
    // Time colon; digits are drawn over the background at runtime.
    b.fill(72, 30, 2, 2, LCD_ON);
    b.fill(72, 35, 2, 2, LCD_ON);
    lcd_box(&mut b, 109, 23, 158, 14);
    lcd_box(&mut b, 109, 41, 19, 10);
    lcd_box(&mut b, 154, 41, 14, 10);
    draw_text(&mut b, 131, 43, "kbps", mix(ICON, PANEL, 0.3));
    draw_text(&mut b, 171, 43, "khz", mix(ICON, PANEL, 0.3));
    lcd_box(&mut b, 211, 40, 56, 14);
    // Separators and the brand mark.
    b.hline(8, 68, 259, PANEL_LO);
    b.hline(8, 69, 259, PANEL_HI);
    b.hline(8, 85, 259, PANEL_LO);
    b.hline(8, 86, 259, PANEL_HI);
    // Brand mark: the logo's "L" of analyzer bars, in pixels.
    b.fill(250, 91, 14, 13, LCD);
    b.bevel(249, 90, 16, 15, PANEL_LO, PANEL_HI);
    for y in 0..9 {
        let c = mix(ACCENT, LCD_ON, y as f32 / 8.0);
        b.fill(252, 93 + y, 2, 1, c);
    }
    for (x, h) in [(255, 3), (258, 4), (261, 2)] {
        for y in 0..h {
            b.set(x, 101 - y, mix(ACCENT, LCD_ON, (h - 1 - y) as f32 / 3.0 + 0.5));
            b.set(x + 1, 101 - y, mix(ACCENT, LCD_ON, (h - 1 - y) as f32 / 3.0 + 0.5));
        }
        b.fill(x, 100 - h, 2, 1, ICON);
    }
    b
}

fn gen_eqmain<P: Pen>() -> P {
    let (w, h) = Sheet::EqMain.size();
    let mut b = P::blank(w, h, PANEL);
    panel(&mut b, 0, 0, 275, 116);
    // Graph, scale labels and band labels.
    draw_text(&mut b, 44, 37, "+12", mix(ICON, PANEL, 0.3));
    draw_text(&mut b, 48, 67, "0", mix(ICON, PANEL, 0.3));
    draw_text(&mut b, 44, 96, "-12", mix(ICON, PANEL, 0.3));
    draw_text(&mut b, 14, 104, "pre", mix(ICON, PANEL, 0.3));
    for (i, label) in ["60", "170", "310", "600", "1k", "3k", "6k", "12k", "14k", "16k"].iter().enumerate() {
        let cx = eq::BAND_X0 + i as i32 * eq::BAND_STEP + 7;
        draw_text(&mut b, cx - text_width(label) / 2, 104, label, mix(ICON, PANEL, 0.3));
    }
    lcd_box(&mut b, 86, 17, 113, 19);
    // Close button and toggles.
    small_button(&mut b, 0, 116, false, "close");
    small_button(&mut b, 0, 125, true, "close");
    for (toggle, label) in [(eq::ON, "on"), (eq::AUTO, "auto")] {
        for on in [false, true] {
            for pressed in [false, true] {
                let rect = toggle.pick(on, pressed);
                button(&mut b, rect, pressed);
                led(&mut b, rect.x + 3, rect.y + 4, on);
                draw_text(&mut b, rect.x + 9, rect.y + 3, label, if on { ICON } else { mix(ICON, BTN_BOT, 0.35) });
            }
        }
    }
    for (rect, active) in [(eq::TITLE_ACTIVE, true), (eq::TITLE_INACTIVE, false)] {
        title_bar(&mut b, rect, "EQUALIZER", active, true);
        small_button(&mut b, rect.x + 254, rect.y + 3, false, "shade");
        small_button(&mut b, rect.x + 264, rect.y + 3, false, "close");
    }
    for (rect, top) in [(eq::THUMB.normal, ACCENT), (eq::THUMB.pressed, mix(ACCENT, ICON, 0.4))] {
        b.vgradient(rect.x, rect.y, rect.w, rect.h, top, ACCENT_LO);
        b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.4), ACCENT_LO);
        b.hline(rect.x + 2, rect.y + 5, 7, ACCENT_LO);
    }
    for k in 0..28 {
        let f = eq::slider_frame(k);
        let t = k as f32 / 27.0;
        b.fill(f.x, f.y, f.w, f.h, PANEL);
        b.fill(f.x + 5, f.y + 1, 4, f.h - 2, PANEL_LO);
        let color = if t < 0.5 { mix(LCD_ON, ACCENT, t * 2.0) } else { mix(ACCENT, rgb(0xe8, 0x4a, 0x3a), (t - 0.5) * 2.0) };
        b.fill(f.x + 6, f.y + 2, 2, f.h - 4, color);
    }
    for (rect, pressed) in [(eq::PRESETS.normal, false), (eq::PRESETS.pressed, true)] {
        button(&mut b, rect, pressed);
        draw_text_centered(&mut b, rect, "presets", ICON);
    }
    let g = eq::GRAPH_BG;
    b.fill(g.x, g.y, g.w, g.h, LCD);
    for i in 0..10 {
        b.vline(g.x + 3 + i * 12, g.y, g.h, LCD_DIM);
    }
    b.hline(g.x, g.y + 9, g.w, LCD_DIM);
    let c = eq::GRAPH_COLORS;
    for y in 0..c.h {
        let t = (y as f32 - 9.0).abs() / 9.0;
        b.set(c.x, c.y + y, mix(LCD_ON, rgb(0xe8, 0x4a, 0x3a), t));
    }
    b.hline(eq::PREAMP_LINE.x, eq::PREAMP_LINE.y, eq::PREAMP_LINE.w, mix(ACCENT, LCD, 0.4));
    b
}

fn gen_eq_ex<P: Pen>() -> P {
    let (w, h) = Sheet::EqEx.size();
    let mut b = P::blank(w, h, PANEL);
    for (rect, active) in [(eq::SHADE_ACTIVE, true), (eq::SHADE_INACTIVE, false)] {
        let (top, bot) = if active { (TITLE_TOP, TITLE_BOT) } else { (TITLE_IN_TOP, TITLE_IN_BOT) };
        b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
        b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.3), PANEL_LO);
        draw_text(&mut b, rect.x + 20, rect.y + 5, "equalizer", if active { ICON } else { mix(ICON, PANEL, 0.5) });
        small_button(&mut b, rect.x + 254, rect.y + 3, false, "unshade");
        small_button(&mut b, rect.x + 264, rect.y + 3, false, "close");
    }
    small_button(&mut b, eq::UNSHADE_PRESSED.x, eq::UNSHADE_PRESSED.y, true, "unshade");
    small_button(&mut b, eq::SHADE_CLOSE_PRESSED.x, eq::SHADE_CLOSE_PRESSED.y, true, "close");
    b
}

fn gen_pledit<P: Pen>() -> P {
    let (w, h) = Sheet::PlEdit.size();
    let mut b = P::blank(w, h, PANEL);
    for (row, active) in [(0usize, true), (1usize, false)] {
        let pick = |p: (Rect, Rect)| if row == 0 { p.0 } else { p.1 };
        let (top, bot) = if active { (TITLE_TOP, TITLE_BOT) } else { (TITLE_IN_TOP, TITLE_IN_BOT) };
        let grip = if active { mix(top, ICON, 0.45) } else { mix(top, ICON, 0.15) };
        for piece in [pl::TOP_LEFT, pl::TITLE, pl::TOP_TILE, pl::TOP_RIGHT] {
            let rect = pick(piece);
            b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
            b.hline(rect.x, rect.y, rect.w, mix(top, ICON, 0.3));
            // Inner frame edge the list sits under.
            b.hline(rect.x, rect.y + rect.h - 2, rect.w, PANEL_LO);
            b.hline(rect.x, rect.y + rect.h - 1, rect.w, PANEL);
        }
        let tl = pick(pl::TOP_LEFT);
        b.vline(tl.x, tl.y, tl.h, mix(top, ICON, 0.3));
        let tile = pick(pl::TOP_TILE);
        for gy in [tile.y + 7, tile.y + 10] {
            b.hline(tile.x, gy, tile.w, grip);
            b.hline(tl.x + 6, gy, tl.w - 6, grip);
        }
        let title = pick(pl::TITLE);
        draw_text_centered(&mut b, r(title.x, title.y, title.w, 18), "PLAYLIST", if active { ICON } else { mix(ICON, PANEL, 0.5) });
        let tr = pick(pl::TOP_RIGHT);
        for gy in [tr.y + 7, tr.y + 10] {
            b.hline(tr.x, gy, 3, grip);
        }
        b.vline(tr.x + tr.w - 1, tr.y, tr.h, PANEL_LO);
        small_button(&mut b, tr.x + 5, tr.y + 3, false, "shade");
        small_button(&mut b, tr.x + 14, tr.y + 3, false, "close");
    }
    small_button(&mut b, pl::CLOSE_PRESSED.x, pl::CLOSE_PRESSED.y, true, "close");
    small_button(&mut b, pl::SHADE_PRESSED.x, pl::SHADE_PRESSED.y, true, "unshade");
    // Side tiles.
    let lt = pl::LEFT_TILE;
    b.vgradient(lt.x, lt.y, lt.w, lt.h, PANEL, PANEL);
    b.vline(lt.x, lt.y, lt.h, mix(PANEL_HI, ICON, 0.2));
    b.vline(lt.x + lt.w - 2, lt.y, lt.h, PANEL_HI);
    b.vline(lt.x + lt.w - 1, lt.y, lt.h, PANEL_LO);
    let rt = pl::RIGHT_TILE;
    b.fill(rt.x, rt.y, rt.w, rt.h, PANEL);
    b.vline(rt.x, rt.y, rt.h, PANEL_HI);
    b.fill(rt.x + 5, rt.y, 8, rt.h, PANEL_LO);
    b.vline(rt.x + rt.w - 1, rt.y, rt.h, PANEL_LO);
    for (rect, top) in [(pl::SCROLL_THUMB.normal, BTN_TOP), (pl::SCROLL_THUMB.pressed, mix(BTN_TOP, ICON, 0.3))] {
        b.vgradient(rect.x, rect.y, rect.w, rect.h, top, BTN_BOT);
        b.bevel(rect.x, rect.y, rect.w, rect.h, mix(top, ICON, 0.3), PANEL_LO);
        for i in 0..3 {
            b.hline(rect.x + 2, rect.y + 7 + i * 2, 4, PANEL_LO);
        }
    }
    // Bottom pieces.
    for rect in [pl::BOTTOM_LEFT, pl::BOTTOM_RIGHT, pl::BOTTOM_TILE] {
        b.vgradient(rect.x, rect.y, rect.w, rect.h, PANEL, mix(PANEL, PANEL_LO, 0.5));
        b.hline(rect.x, rect.y, rect.w, PANEL_HI);
        b.hline(rect.x, rect.y + rect.h - 1, rect.w, PANEL_LO);
    }
    b.vline(pl::BOTTOM_LEFT.x, pl::BOTTOM_LEFT.y, pl::BOTTOM_LEFT.h, mix(PANEL_HI, ICON, 0.2));
    for (i, label) in ["add", "rem", "sel", "misc"].iter().enumerate() {
        let rect = r(pl::BOTTOM_LEFT.x + 14 + i as i32 * 29, pl::BOTTOM_LEFT.y + 8, 25, 18);
        button(&mut b, rect, false);
        draw_text_centered(&mut b, rect, label, ICON);
    }
    let br = pl::BOTTOM_RIGHT;
    lcd_box(&mut b, br.x + 7, br.y + 8, 90, 11);
    let list = r(br.x + 106, br.y + 8, 22, 18);
    button(&mut b, list, false);
    draw_text_centered(&mut b, list, "list", ICON);
    // Mini transport (prev, play, pause, stop, next, eject) and mini time.
    let mini = [(6, 8), (14, 8), (22, 9), (31, 8), (39, 8), (48, 9)];
    for (i, (mx, mw)) in mini.iter().enumerate() {
        let rect = r(br.x + mx, br.y + 22, *mw, 8);
        let (cx, cy) = (rect.x as f32 + rect.w as f32 / 2.0, rect.y as f32 + 4.0);
        let c = mix(ICON, PANEL, 0.2);
        match i {
            0 => {
                b.vline(rect.x + 1, rect.y + 1, 6, c);
                b.polygon(&[(cx + 3.0, cy - 3.0), (cx + 3.0, cy + 3.0), (cx - 2.0, cy)], c);
            }
            1 => b.polygon(&[(cx - 2.0, cy - 3.5), (cx - 2.0, cy + 3.5), (cx + 3.0, cy)], c),
            2 => {
                b.fill(rect.x + 2, rect.y + 1, 2, 6, c);
                b.fill(rect.x + 5, rect.y + 1, 2, 6, c);
            }
            3 => b.fill(rect.x + 1, rect.y + 1, 6, 6, c),
            4 => {
                b.polygon(&[(cx - 3.0, cy - 3.0), (cx - 3.0, cy + 3.0), (cx + 2.0, cy)], c);
                b.vline(rect.x + rect.w - 2, rect.y + 1, 6, c);
            }
            _ => {
                b.polygon(&[(cx - 3.5, cy + 1.0), (cx + 3.5, cy + 1.0), (cx, cy - 3.0)], c);
                b.hline(rect.x + 1, rect.y + 6, 7, c);
            }
        }
    }
    lcd_box(&mut b, br.x + 66, br.y + 23, 32, 8);
    // Resize grip.
    for i in 0..4 {
        for j in 0..=i {
            b.set(br.x + 145 - j * 3, br.y + 34 - (i - j) * 3, mix(ICON, PANEL, 0.5));
        }
    }
    let vis = pl::VIS;
    b.vgradient(vis.x, vis.y, vis.w, vis.h, PANEL, mix(PANEL, PANEL_LO, 0.5));
    b.hline(vis.x, vis.y, vis.w, PANEL_HI);
    lcd_box(&mut b, vis.x + 3, vis.y + 10, 72, 18);
    // Shade bars.
    for (piece, active) in [(0usize, true), (1usize, false)] {
        let (top, bot) = if active { (TITLE_TOP, TITLE_BOT) } else { (TITLE_IN_TOP, TITLE_IN_BOT) };
        let left = if piece == 0 { pl::SHADE_LEFT.0 } else { pl::SHADE_LEFT.1 };
        let right = if piece == 0 { pl::SHADE_RIGHT.0 } else { pl::SHADE_RIGHT.1 };
        for rect in [left, right] {
            b.vgradient(rect.x, rect.y, rect.w, rect.h, top, bot);
            b.hline(rect.x, rect.y, rect.w, mix(top, ICON, 0.3));
            b.hline(rect.x, rect.y + rect.h - 1, rect.w, PANEL_LO);
        }
        small_button(&mut b, right.x + 32, right.y + 3, false, "unshade");
        small_button(&mut b, right.x + 41, right.y + 3, false, "close");
    }
    // Pop-up menu buttons.
    let labels: [&[(&str, &str)]; 5] = [
        &[("add", "file"), ("add", "dir"), ("add", "url")],
        &[("rem", "misc"), ("rem", "sel"), ("crop", ""), ("rem", "all")],
        &[("sel", "all"), ("sel", "zero"), ("inv", "sel")],
        &[("misc", "opts"), ("file", "info"), ("sort", "list")],
        &[("load", "list"), ("save", "list"), ("new", "list")],
    ];
    for (m, items) in labels.iter().enumerate() {
        for (k, (top_line, bottom_line)) in items.iter().enumerate() {
            let (normal, pressed) = pl::menu_items(m)[k];
            for (rect, is_pressed) in [(normal, false), (pressed, true)] {
                button(&mut b, rect, is_pressed);
                if bottom_line.is_empty() {
                    draw_text_centered(&mut b, rect, top_line, ICON);
                } else {
                    draw_text_centered(&mut b, r(rect.x, rect.y + 1, rect.w, 8), top_line, ICON);
                    draw_text_centered(&mut b, r(rect.x, rect.y + 9, rect.w, 8), bottom_line, ICON);
                }
            }
        }
        let bar = pl::MENU_BARS[m];
        b.fill(bar.x, bar.y, bar.w, bar.h, ACCENT_LO);
        b.vline(bar.x + 1, bar.y, bar.h, ACCENT);
    }
    b
}

pub fn sheet(s: Sheet) -> Bitmap {
    draw(s)
}

/// The sheet as an SVG drawing, for the vector version of the skin.
pub fn svg_sheet(s: Sheet) -> String {
    draw::<crate::pen::Svg>(s).finish()
}

fn draw<P: Pen>(sheet: Sheet) -> P {
    match sheet {
        Sheet::Main => gen_main(),
        Sheet::CButtons => gen_cbuttons(),
        Sheet::TitleBar => gen_titlebar(),
        Sheet::ShufRep => gen_shufrep(),
        Sheet::Text => gen_text(),
        Sheet::Volume => gen_slider(false),
        Sheet::Balance => gen_slider(true),
        Sheet::MonoSter => gen_monoster(),
        Sheet::PlayPaus => gen_playpaus(),
        Sheet::Numbers => gen_numbers(false),
        Sheet::NumsEx => gen_numbers(true),
        Sheet::PosBar => gen_posbar(),
        Sheet::EqMain => gen_eqmain(),
        Sheet::EqEx => gen_eq_ex(),
        Sheet::PlEdit => gen_pledit(),
    }
}

pub fn viscolors() -> [u32; 24] {
    let mut c = [0u32; 24];
    c[0] = LCD;
    c[1] = rgb(0x12, 0x1c, 0x22);
    for i in 0..16 {
        // 2 = top of the bars (hot) .. 17 = bottom (cool)
        let t = i as f32 / 15.0;
        c[2 + i] = if t < 0.5 { mix(rgb(0xf0, 0x50, 0x3c), ACCENT, t * 2.0) } else { mix(ACCENT, LCD_ON, (t - 0.5) * 2.0) };
    }
    for i in 0..5 {
        c[18 + i] = mix(ICON, LCD_ON, i as f32 / 4.0);
    }
    c[23] = ICON;
    c
}

pub fn pl_colors() -> PlColors {
    PlColors { normal: LCD_ON, current: ICON, normal_bg: LCD, selected_bg: rgb(0x23, 0x3a, 0x6b), font: "Sans".into() }
}
