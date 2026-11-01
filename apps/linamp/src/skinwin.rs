//! A skinned window run from its layout: draws the elements of the current
//! mode and turns mouse input into commands and value changes. The same
//! code serves every window of every skin.

use crate::bind;
use crate::model::Model;
use crate::render::{skin_text, skin_text_width, sprite, time_digits};
use crate::ui::{Input, Out};
use linamp_audio::PlayState;
use linamp_core::text::format_duration_ms;
use linamp_skin::layout::{Action, Element, EmptyPress, Fill, Kind, MenuRef, Mode, Paint, Slider, Value, Wheel, WindowDef, WindowKey};
use linamp_skin::sprites::{eq as e, main as m, Rect};
use linamp_skin::{Bitmap, CursorKind, Sheet, Skin};
use std::time::{Duration, Instant};

const MARQUEE_STEP: Duration = Duration::from_millis(180);

pub struct SkinWin {
    key: WindowKey,
    /// The element under the held mouse button: (shade mode, index).
    pressed: Option<(bool, usize)>,
    /// Where on the thumb a slider was taken, along the slider.
    grab: i32,
    /// The value of a slider that applies on release, while it is dragged.
    preview: Option<i64>,
    marquee_offset: i32,
    marquee_drag: Option<(i32, i32)>,
    last_scroll: Instant,
}

/// The window's current mode with its size.
struct Current<'a> {
    mode: &'a Mode,
    w: i32,
    h: i32,
}

impl SkinWin {
    pub fn new(key: WindowKey) -> Self {
        Self { key, pressed: None, grab: 0, preview: None, marquee_offset: 0, marquee_drag: None, last_scroll: Instant::now() }
    }

    fn def<'a>(&self, model: &'a Model) -> Option<&'a WindowDef> {
        model.skin.layout.window(self.key)
    }

    fn current<'a>(&self, model: &'a Model) -> Option<Current<'a>> {
        let shade = bind::shaded(model, self.key);
        let mode = self.def(model)?.mode(shade);
        let (w, h) = bind::window_size(model, self.key);
        Some(Current { mode, w, h })
    }

    /// The pressed element, if it belongs to the mode on screen.
    fn pressed_index(&self, model: &Model) -> Option<usize> {
        let (shade, i) = self.pressed?;
        (shade == bind::shaded(model, self.key)).then_some(i)
    }

    /// The last element under the point that takes the mouse.
    fn hit(&self, model: &Model, x: i32, y: i32) -> Option<usize> {
        let c = self.current(model)?;
        c.mode.elements.iter().enumerate().rev().find_map(|(i, el)| {
            let takes = el.interactive() && bind::holds(model, el.show, c.w) && el.at.resolve(c.w, c.h).contains(x, y);
            takes.then_some(i)
        })
    }

    fn element(&self, model: &Model, i: usize) -> Option<(Element, Rect)> {
        let c = self.current(model)?;
        let el = c.mode.elements.get(i)?;
        Some((el.clone(), el.at.resolve(c.w, c.h)))
    }

    /// The skin cursor for the element under the mouse, or the one being dragged.
    pub fn cursor(&self, model: &Model, x: i32, y: i32) -> CursorKind {
        let Some(c) = self.current(model) else { return CursorKind::Normal };
        self.pressed_index(model)
            .or_else(|| self.hit(model, x, y))
            .and_then(|i| c.mode.elements.get(i))
            .and_then(|el| el.cursor)
            .unwrap_or(c.mode.cursor)
    }

    pub fn tooltip(&self, model: &Model, x: i32, y: i32) -> Option<String> {
        let c = self.current(model)?;
        self.hit(model, x, y).and_then(|i| c.mode.elements[i].tip.clone())
    }

    /// The play position being chosen on a slider, if any.
    fn seek_preview(&self, model: &Model) -> Option<i64> {
        let (el, _) = self.element(model, self.pressed_index(model)?)?;
        match el.kind {
            Kind::Slider(Slider { value: Value::Position, live: false, .. }) => self.preview,
            _ => None,
        }
    }

    fn marquee_text(&self, model: &Model) -> String {
        if let Some(ms) = self.seek_preview(model) {
            let len = model.engine.shared().length_ms().unwrap_or(1).max(1);
            let ms = ms.max(0) as u64;
            return format!("SEEK TO: {}/{} ({}%)", format_duration_ms(ms), format_duration_ms(len), ms * 100 / len);
        }
        if let Some((t, until)) = &model.transient {
            if Instant::now() < *until {
                return t.clone();
            }
        }
        let title = model.current_title();
        if title.is_empty() {
            format!("LINAMP {}", env!("CARGO_PKG_VERSION"))
        } else {
            title
        }
    }

    fn marquee_width(&self, model: &Model) -> Option<i32> {
        let c = self.current(model)?;
        c.mode.elements.iter().find_map(|el| match el.kind {
            Kind::Marquee { area, .. } => Some(area.w),
            _ => None,
        })
    }

    /// Advances the marquee; returns true if it moved.
    pub fn tick(&mut self, model: &Model) -> bool {
        let Some(width) = self.marquee_width(model) else { return false };
        if self.marquee_drag.is_some() || !model.settings().autoscrollname {
            return false;
        }
        if skin_text_width(&self.marquee_text(model)) <= width {
            self.marquee_offset = 0;
            return false;
        }
        if self.last_scroll.elapsed() >= MARQUEE_STEP {
            self.last_scroll = Instant::now();
            self.marquee_offset += 5;
            return true;
        }
        false
    }

    // --- drawing -------------------------------------------------------------

    pub fn draw(&self, model: &Model, b: &mut Bitmap, active: bool) {
        let Some(c) = self.current(model) else { return };
        let active = active || !model.settings().dim_inactive;
        let pressed = self.pressed_index(model);
        for (i, el) in c.mode.elements.iter().enumerate() {
            if !bind::holds(model, el.show, c.w) {
                continue;
            }
            let r = el.at.resolve(c.w, c.h);
            let held = pressed == Some(i);
            let skin = &model.skin;
            match &el.kind {
                Kind::Image { look, fill, lit } => {
                    let on = lit.is_some_and(|f| bind::flag(model, f));
                    if let Some(p) = look.pick(on, false, active) {
                        draw_paint(b, skin, p, r, *fill);
                    }
                }
                Kind::Button { look, .. } => {
                    if let Some(p) = look.pick(false, held, active) {
                        draw_paint(b, skin, p, r, Fill::Once);
                    }
                }
                Kind::Toggle { look, flag, .. } => {
                    if let Some(p) = look.pick(bind::flag(model, *flag), held, active) {
                        draw_paint(b, skin, p, r, Fill::Once);
                    }
                }
                Kind::Slider(s) => self.draw_slider(model, b, s, r, held),
                Kind::Text(t) => {
                    if let Some(text) = bind::text(model, *t) {
                        skin_text(b, skin, r, &text, 0);
                    }
                }
                Kind::Marquee { area, .. } => self.draw_marquee(model, b, Rect { x: r.x + area.x, y: r.y + area.y, ..*area }),
                Kind::Clock => self.draw_clock(model, b, r),
                Kind::Vis => model.vis.draw(b, r.x, r.y, r.w, r.h, model.settings(), &skin.viscolor),
                Kind::EqGraph => draw_eq_graph(model, b, r),
                Kind::Drag { .. } | Kind::Resize | Kind::List | Kind::ListScroll { .. } | Kind::Popup { .. } => {}
            }
        }
    }

    fn draw_slider(&self, model: &Model, b: &mut Bitmap, s: &Slider, r: Rect, held: bool) {
        let Some((v, min, max)) = bind::value(model, s.value) else { return };
        let shown = if held && !s.live { self.preview.unwrap_or(v) } else { v };
        if let Some(f) = &s.track {
            sprite(b, &model.skin, f.sheet, f.frame(shown, min, max), r.x, r.y);
        }
        if let Some(p) = s.thumb.pick(false, held, true) {
            let off = thumb_offset(s, r, shown, min, max);
            let (x, y) = if s.vertical { (r.x, r.y + off) } else { (r.x + off, r.y) };
            draw_paint(b, &model.skin, p, Rect { x, y, ..r }, Fill::Once);
        }
    }

    fn draw_marquee(&self, model: &Model, b: &mut Bitmap, area: Rect) {
        let text = self.marquee_text(model);
        if skin_text_width(&text) > area.w {
            let full = format!("{text}  ***  ");
            let period = skin_text_width(&full);
            let off = self.marquee_offset.rem_euclid(period);
            skin_text(b, &model.skin, area, &format!("{full}{full}"), off);
        } else {
            skin_text(b, &model.skin, area, &text, 0);
        }
    }

    /// The big time display; paused time blinks once a second.
    fn draw_clock(&self, model: &Model, b: &mut Bitmap, r: Rect) {
        let sh = model.engine.shared();
        let blink_off = sh.state() == PlayState::Paused
            && (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() / 500) % 2 == 1;
        if blink_off {
            return;
        }
        let shown = self.seek_preview(model).map(|p| p.max(0) as u64).unwrap_or_else(|| sh.position_ms());
        let (secs, neg) = match (model.settings().timeleftmode, sh.length_ms()) {
            (true, Some(l)) => (l.saturating_sub(shown) / 1000, true),
            _ => (shown / 1000, false),
        };
        time_digits(b, &model.skin, Some(secs), neg, r.x - m::TIME_AREA.x, r.y - m::TIME_AREA.y);
    }

    // --- input ---------------------------------------------------------------

    pub fn mouse_down(&mut self, model: &mut Model, ev: &Input) -> Out {
        let hit = self.hit(model, ev.x, ev.y);
        let found = hit.and_then(|i| self.element(model, i));
        if ev.right {
            let menu = found.and_then(|(el, _)| el.menu).unwrap_or(MenuRef::Main);
            return Out::Menu(bind::menu(model, menu));
        }
        let (Some(i), Some((el, r))) = (hit, found) else { return self.empty_press(model) };
        let shade = bind::shaded(model, self.key);
        match el.kind {
            Kind::Drag { double } => {
                if let (true, Some(c)) = (ev.double, double) {
                    model.exec(c);
                    return Out::None;
                }
                Out::BeginMove
            }
            Kind::Button { action, .. } | Kind::Toggle { action, .. } => match action {
                Action::Press(c) => {
                    model.exec(c);
                    Out::Redraw
                }
                Action::MenuOnPress(menu) => Out::Menu(bind::menu(model, menu)),
                _ => {
                    self.pressed = Some((shade, i));
                    Out::Capture
                }
            },
            Kind::Slider(s) => {
                if let (true, Some(v)) = (ev.double, s.reset) {
                    bind::set_value(model, s.value, v);
                    return Out::Redraw;
                }
                let Some((v, min, max)) = bind::value(model, s.value) else { return Out::None };
                let (p, start) = if s.vertical { (ev.y, r.y) } else { (ev.x, r.x) };
                let thumb = start + thumb_offset(&s, r, v, min, max);
                self.grab = if p >= thumb && p < thumb + s.thumb_len { p - thumb } else { s.thumb_len / 2 };
                self.pressed = Some((shade, i));
                self.slide(model, &s, r, ev.x, ev.y);
                Out::Capture
            }
            Kind::Marquee { double, .. } => {
                if let (true, Some(c)) = (ev.double, double) {
                    model.exec(c);
                    return Out::None;
                }
                self.marquee_drag = Some((ev.x, self.marquee_offset));
                self.pressed = Some((shade, i));
                Out::Capture
            }
            Kind::Resize => Out::BeginResize,
            _ => Out::None,
        }
    }

    fn empty_press(&self, model: &Model) -> Out {
        match self.def(model).map(|d| d.empty_press) {
            Some(EmptyPress::Move) => Out::BeginMove,
            Some(EmptyPress::MoveIfEasy) if model.settings().easymove => Out::BeginMove,
            _ => Out::None,
        }
    }

    /// Moves a slider to the pointer.
    fn slide(&mut self, model: &mut Model, s: &Slider, r: Rect, x: i32, y: i32) {
        let Some((v, min, max)) = bind::value(model, s.value) else { return };
        let (p, start, len) = if s.vertical { (y, r.y, r.h) } else { (x, r.x, r.w) };
        let range = (len - s.thumb_len).max(1) as i64;
        let t = ((p - start - self.grab) as i64).clamp(0, range);
        let mut to = min + (2 * t * (max - min) + range) / (2 * range);
        let mid = (min + max) / 2;
        if (to - mid).abs() < s.detent {
            to = mid;
        }
        if s.live {
            if to != v {
                bind::set_value(model, s.value, to);
            }
        } else {
            self.preview = Some(to);
        }
    }

    /// Follows a drag; returns true if the window needs drawing.
    pub fn mouse_move(&mut self, model: &mut Model, x: i32, y: i32) -> bool {
        let Some((el, r)) = self.pressed_index(model).and_then(|i| self.element(model, i)) else { return false };
        match el.kind {
            Kind::Slider(s) => self.slide(model, &s, r, x, y),
            Kind::Marquee { .. } => {
                let Some((sx, so)) = self.marquee_drag else { return false };
                self.marquee_offset = so - (x - sx);
            }
            _ => return false,
        }
        true
    }

    pub fn mouse_up(&mut self, model: &mut Model, ev: &Input) -> Out {
        let Some(i) = self.pressed_index(model) else {
            self.pressed = None;
            return Out::None;
        };
        let over = self.hit(model, ev.x, ev.y) == Some(i);
        self.pressed = None;
        let Some((el, _)) = self.element(model, i) else { return Out::Redraw };
        match el.kind {
            Kind::Slider(s) => {
                if let Some(v) = self.preview.take() {
                    bind::set_value(model, s.value, v);
                }
                Out::Redraw
            }
            Kind::Marquee { .. } => {
                self.marquee_drag = None;
                self.last_scroll = Instant::now() + Duration::from_secs(1);
                Out::Redraw
            }
            Kind::Button { action, .. } | Kind::Toggle { action, .. } if over => match action {
                Action::Click { plain, shift, ctrl } => {
                    let c = match (ev.ctrl, ev.shift) {
                        (true, _) => ctrl.unwrap_or(plain),
                        (false, true) => shift.unwrap_or(plain),
                        _ => plain,
                    };
                    model.exec(c);
                    Out::Redraw
                }
                Action::MenuOnRelease(menu) => Out::Menu(bind::menu(model, menu)),
                _ => Out::Redraw,
            },
            _ => Out::Redraw,
        }
    }

    pub fn wheel(&mut self, model: &mut Model, x: i32, y: i32, steps: i32) {
        if let Some((Element { kind: Kind::Slider(s), .. }, _)) = self.hit(model, x, y).and_then(|i| self.element(model, i)) {
            if s.wheel != 0 {
                if let Some((v, _, _)) = bind::value(model, s.value) {
                    bind::set_value(model, s.value, v + s.wheel * steps as i64);
                }
                return;
            }
        }
        if let Some(Wheel::Volume(step)) = self.def(model).map(|d| d.wheel) {
            model.set_volume(model.settings().volume + steps * step);
        }
    }
}

/// How far the thumb sits from the slider's start for value `v`.
fn thumb_offset(s: &Slider, r: Rect, v: i64, min: i64, max: i64) -> i32 {
    let len = if s.vertical { r.h } else { r.w };
    let range = (len - s.thumb_len).max(0) as i64;
    ((v - min).clamp(0, (max - min).max(0)) * range / (max - min).max(1)) as i32
}

fn draw_paint(b: &mut Bitmap, skin: &Skin, p: Paint, r: Rect, fill: Fill) {
    match fill {
        Fill::Once => sprite(b, skin, p.sheet, p.src, r.x + p.dx, r.y + p.dy),
        Fill::Tile => {
            let (tw, th) = (p.src.w.max(1), p.src.h.max(1));
            let mut y = r.y;
            while y < r.y + r.h {
                let mut x = r.x;
                while x < r.x + r.w {
                    let (w, h) = (tw.min(r.x + r.w - x), th.min(r.y + r.h - y));
                    b.blit(skin.sheet(p.sheet), p.src.x, p.src.y, w, h, x, y);
                    x += tw;
                }
                y += th;
            }
        }
    }
}

/// Spline through the band values, coloured from the skin's colour strip.
fn draw_eq_graph(model: &Model, b: &mut Bitmap, r: Rect) {
    let skin = &model.skin;
    let (gx, gy) = (r.x, r.y);
    sprite(b, skin, Sheet::EqMain, e::GRAPH_BG, gx, gy);
    let preamp_y = gy + 1 + model.settings().preamp * 16 / 63;
    sprite(b, skin, Sheet::EqMain, e::PREAMP_LINE, gx, preamp_y);
    let bands = model.settings().eq_bands();
    let ys: Vec<f32> = bands.iter().map(|&v| v as f32 * 18.0 / 63.0).collect();
    let colors = skin.sheet(Sheet::EqMain);
    let mut prev: Option<i32> = None;
    for px in 0..113 {
        // Catmull-Rom over the 10 points spread across 113 px.
        let t = px as f32 / 112.0 * 9.0;
        let i = (t.floor() as usize).min(8);
        let f = t - i as f32;
        let p = |k: isize| ys[(i as isize + k).clamp(0, 9) as usize];
        let (p0, p1, p2, p3) = (p(-1), p(0), p(1), p(2));
        let y = 0.5
            * (2.0 * p1 + (-p0 + p2) * f + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f * f + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f * f * f);
        let y = (y.round() as i32).clamp(0, 18);
        let (lo, hi) = match prev {
            Some(py) => (py.min(y), py.max(y)),
            None => (y, y),
        };
        for yy in lo..=hi {
            let c = colors.get(e::GRAPH_COLORS.x, e::GRAPH_COLORS.y + yy).unwrap_or(0xffffff);
            b.set(gx + px, gy + yy, c);
        }
        prev = Some(y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linamp_skin::CursorKind as C;

    fn input(x: i32, y: i32) -> Input {
        Input { x, y, right: false, shift: false, ctrl: false, double: false }
    }

    #[test]
    fn main_cursors_follow_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = SkinWin::new(WindowKey::Main);
        let at = |m: &Model, x, y| w.cursor(m, x, y);
        assert_eq!(at(&m, 9, 5), C::MainMenu);
        assert_eq!(at(&m, 247, 5), C::Min);
        assert_eq!(at(&m, 267, 5), C::Close);
        assert_eq!(at(&m, 257, 5), C::WinBut);
        assert_eq!(at(&m, 100, 5), C::TitleBar);
        assert_eq!(at(&m, 120, 60), C::VolBal);
        assert_eq!(at(&m, 180, 27), C::SongName);
        assert_eq!(at(&m, 50, 97), C::Normal);
        m.settings_mut().windowshade = true;
        assert_eq!(at(&m, 9, 5), C::MMenu);
        assert_eq!(at(&m, 100, 5), C::WSNormal);
    }

    #[test]
    fn main_tooltips_name_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = SkinWin::new(WindowKey::Main);
        assert_eq!(w.tooltip(&m, 13, 36).as_deref(), Some("Toggle Always-On-Top"));
        assert_eq!(w.tooltip(&m, 180, 27).as_deref(), Some("Song Title (right-click for options)"));
        assert_eq!(w.tooltip(&m, 9, 5).as_deref(), Some("Main Menu"));
        assert_eq!(w.tooltip(&m, 50, 97), None);
        // The clutter bar is hidden in windowshade mode.
        m.settings_mut().windowshade = true;
        assert_eq!(w.tooltip(&m, 13, 36), None);
    }

    #[test]
    fn clicks_run_the_button_command_on_release() {
        let mut m = Model::for_tests();
        let mut w = SkinWin::new(WindowKey::Main);
        // Shuffle toggle at (164, 89).
        assert!(matches!(w.mouse_down(&mut m, &input(170, 95)), Out::Capture));
        assert!(!m.settings().shuffle, "nothing happens before release");
        w.mouse_up(&mut m, &input(170, 95));
        assert!(m.settings().shuffle);
        // Released away from the button: nothing.
        w.mouse_down(&mut m, &input(170, 95));
        w.mouse_up(&mut m, &input(10, 100));
        assert!(m.settings().shuffle);
    }

    #[test]
    fn volume_follows_the_drag_and_balance_snaps_to_centre() {
        let mut m = Model::for_tests();
        let mut w = SkinWin::new(WindowKey::Main);
        // Volume: 68 px wide at x 107, 14 px thumb; grabbed off the thumb at its centre.
        w.mouse_down(&mut m, &input(107 + 7, 60));
        assert_eq!(m.settings().volume, 0);
        w.mouse_move(&mut m, 107 + 7 + 54, 60);
        assert_eq!(m.settings().volume, 255);
        w.mouse_move(&mut m, 107 + 7 + 27, 60);
        assert_eq!(m.settings().volume, 128);
        w.mouse_up(&mut m, &input(0, 0));
        // Balance at x 177, 38 px: a little off centre snaps to it.
        m.set_balance(-127);
        w.mouse_down(&mut m, &input(177 + 7 + 13, 60));
        assert_eq!(m.settings().balance, 0);
        w.mouse_up(&mut m, &input(0, 0));
    }

    #[test]
    fn wheel_changes_the_volume_by_default() {
        let mut m = Model::for_tests();
        m.set_volume(100);
        let mut w = SkinWin::new(WindowKey::Main);
        w.wheel(&mut m, 50, 50, 2);
        assert_eq!(m.settings().volume, 116);
    }

    #[test]
    fn eq_cursors_follow_the_control_under_the_mouse() {
        let m = Model::for_tests();
        let w = SkinWin::new(WindowKey::Eq);
        assert_eq!(w.cursor(&m, 267, 5), C::EqClose);
        assert_eq!(w.cursor(&m, 25, 50), C::EqSlid);
        assert_eq!(w.cursor(&m, 82, 50), C::EqSlid);
        assert_eq!(w.cursor(&m, 100, 5), C::EqTitle);
        assert_eq!(w.cursor(&m, 5, 105), C::EqNormal);
    }

    #[test]
    fn eq_sliders_drag_reset_and_wheel() {
        let mut m = Model::for_tests();
        let mut w = SkinWin::new(WindowKey::Eq);
        // Band 0 at x 78, y 38, 63 px tall, 11 px thumb: top is +12 dB (0).
        w.mouse_down(&mut m, &input(84, 38 + 5));
        w.mouse_move(&mut m, 84, 38);
        assert_eq!(m.settings().eq_bands()[0], 0);
        w.mouse_move(&mut m, 84, 38 + 5 + 52);
        assert_eq!(m.settings().eq_bands()[0], 63);
        w.mouse_up(&mut m, &input(84, 100));
        w.mouse_down(&mut m, &Input { double: true, ..input(84, 60) });
        assert_eq!(m.settings().eq_bands()[0], 31, "double click resets");
        w.wheel(&mut m, 84, 60, 1);
        assert_eq!(m.settings().eq_bands()[0], 29, "wheel up raises the band");
        // Empty space moves the window.
        assert!(matches!(w.mouse_down(&mut m, &input(5, 105)), Out::BeginMove));
    }
}
