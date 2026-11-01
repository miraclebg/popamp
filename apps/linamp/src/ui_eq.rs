//! The equalizer window.

use crate::model::Model;
use crate::render::sprite;
use crate::ui::{Input, Out};
use linamp_core::Command;
use linamp_skin::sprites::{eq as e, r, Rect};
use linamp_skin::CursorKind;
use linamp_skin::{Bitmap, Sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum W {
    Close,
    Shade,
    On,
    Auto,
    Presets,
    Preamp,
    Band(usize),
    Title,
}

pub struct EqWin {
    pressed: Option<W>,
}

const THUMB_RANGE: i32 = e::SLIDER_H - 11;

fn slider_rect(w: W) -> Option<Rect> {
    match w {
        W::Preamp => Some(r(e::AT_PREAMP.0, e::AT_PREAMP.1, e::SLIDER_W, e::SLIDER_H)),
        W::Band(i) => Some(r(e::BAND_X0 + i as i32 * e::BAND_STEP, e::BAND_Y, e::SLIDER_W, e::SLIDER_H)),
        _ => None,
    }
}

impl EqWin {
    pub fn new() -> Self {
        Self { pressed: None }
    }

    pub fn size(model: &Model) -> (i32, i32) {
        if model.settings().eq_ws {
            e::SHADE_SIZE
        } else {
            e::SIZE
        }
    }

    /// The skin cursor for the control under the mouse (or being dragged).
    pub fn cursor(&self, model: &Model, x: i32, y: i32) -> CursorKind {
        match self.pressed.or_else(|| self.hit(model, x, y)) {
            Some(W::Close) => CursorKind::EqClose,
            Some(W::Preamp | W::Band(_)) => CursorKind::EqSlid,
            Some(W::Title) => CursorKind::EqTitle,
            _ => CursorKind::EqNormal,
        }
    }

    fn hit(&self, model: &Model, x: i32, y: i32) -> Option<W> {
        let close = r(e::AT_CLOSE.0, e::AT_CLOSE.1, 9, 9);
        let shade = r(e::AT_SHADE.0, e::AT_SHADE.1, 9, 9);
        if close.contains(x, y) {
            return Some(W::Close);
        }
        if shade.contains(x, y) {
            return Some(W::Shade);
        }
        if model.settings().eq_ws {
            return Some(W::Title);
        }
        let items = [
            (r(e::AT_ON.0, e::AT_ON.1, 26, 12), W::On),
            (r(e::AT_AUTO.0, e::AT_AUTO.1, 32, 12), W::Auto),
            (r(e::AT_PRESETS.0, e::AT_PRESETS.1, 44, 12), W::Presets),
        ];
        for (rect, w) in items {
            if rect.contains(x, y) {
                return Some(w);
            }
        }
        for w in std::iter::once(W::Preamp).chain((0..10).map(W::Band)) {
            if slider_rect(w).is_some_and(|rc| rc.contains(x, y)) {
                return Some(w);
            }
        }
        (y < 14).then_some(W::Title)
    }

    fn value(model: &Model, w: W) -> i32 {
        match w {
            W::Preamp => model.settings().preamp,
            W::Band(i) => model.settings().eq_bands()[i] as i32,
            _ => 31,
        }
    }

    fn set_from_y(model: &mut Model, w: W, y: i32) {
        let Some(rc) = slider_rect(w) else { return };
        let v = ((y - rc.y - 5) * 63 + THUMB_RANGE / 2) / THUMB_RANGE;
        match w {
            W::Preamp => model.set_preamp(v),
            W::Band(i) => model.set_eq_band(i, v),
            _ => {}
        }
    }

    pub fn draw(&mut self, model: &mut Model, b: &mut Bitmap, active: bool) {
        let skin = &model.skin;
        let s = model.settings();
        let active = active || !s.dim_inactive;
        if s.eq_ws {
            sprite(b, skin, Sheet::EqEx, if active { e::SHADE_ACTIVE } else { e::SHADE_INACTIVE }, 0, 0);
            if self.pressed == Some(W::Shade) {
                sprite(b, skin, Sheet::EqEx, e::UNSHADE_PRESSED, e::AT_SHADE.0, e::AT_SHADE.1);
            }
            if self.pressed == Some(W::Close) {
                sprite(b, skin, Sheet::EqEx, e::SHADE_CLOSE_PRESSED, e::AT_CLOSE.0, e::AT_CLOSE.1);
            }
            return;
        }
        sprite(b, skin, Sheet::EqMain, e::BG, 0, 0);
        sprite(b, skin, Sheet::EqMain, if active { e::TITLE_ACTIVE } else { e::TITLE_INACTIVE }, 0, 0);
        if self.pressed == Some(W::Close) {
            sprite(b, skin, Sheet::EqMain, e::CLOSE.pressed, e::AT_CLOSE.0, e::AT_CLOSE.1);
        }
        sprite(b, skin, Sheet::EqMain, e::ON.pick(s.use_eq, self.pressed == Some(W::On)), e::AT_ON.0, e::AT_ON.1);
        sprite(b, skin, Sheet::EqMain, e::AUTO.pick(s.autoload_eq, self.pressed == Some(W::Auto)), e::AT_AUTO.0, e::AT_AUTO.1);
        let presets = if self.pressed == Some(W::Presets) { e::PRESETS.pressed } else { e::PRESETS.normal };
        sprite(b, skin, Sheet::EqMain, presets, e::AT_PRESETS.0, e::AT_PRESETS.1);
        // Sliders.
        for w in std::iter::once(W::Preamp).chain((0..10).map(W::Band)) {
            let rc = slider_rect(w).expect("slider widget");
            let v = Self::value(model, w);
            let frame = ((63 - v) * 27 + 31) / 63;
            sprite(b, skin, Sheet::EqMain, e::slider_frame(frame), rc.x, rc.y);
            let ty = rc.y + v * THUMB_RANGE / 63;
            let thumb = if self.pressed == Some(w) { e::THUMB.pressed } else { e::THUMB.normal };
            sprite(b, skin, Sheet::EqMain, thumb, rc.x + 1, ty);
        }
        self.draw_graph(model, b);
    }

    /// Spline through the band values, coloured from the skin's colour strip.
    fn draw_graph(&self, model: &Model, b: &mut Bitmap) {
        let skin = &model.skin;
        let (gx, gy) = e::AT_GRAPH;
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
                * (2.0 * p1
                    + (-p0 + p2) * f
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f * f
                    + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f * f * f);
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

    pub fn mouse_down(&mut self, model: &mut Model, ev: &Input) -> Out {
        let w = self.hit(model, ev.x, ev.y);
        if ev.right {
            return Out::Menu(match w {
                Some(W::Band(_)) | Some(W::Preamp) | Some(W::Presets) => model.eq_presets_menu(),
                _ => model.main_menu(),
            });
        }
        match w {
            None | Some(W::Title) => {
                if ev.double {
                    model.exec(Command::ToggleEqShade);
                    return Out::None;
                }
                Out::BeginMove
            }
            Some(W::Presets) => Out::Menu(model.eq_presets_menu()),
            Some(sl @ (W::Preamp | W::Band(_))) => {
                if ev.double {
                    // Double-click resets a slider to flat.
                    match sl {
                        W::Preamp => model.set_preamp(31),
                        W::Band(i) => model.set_eq_band(i, 31),
                        _ => {}
                    }
                    return Out::Redraw;
                }
                self.pressed = w;
                Self::set_from_y(model, sl, ev.y);
                Out::Capture
            }
            Some(other) => {
                self.pressed = Some(other);
                Out::Capture
            }
        }
    }

    pub fn mouse_move(&mut self, model: &mut Model, _x: i32, y: i32) -> bool {
        match self.pressed {
            Some(w @ (W::Preamp | W::Band(_))) => {
                Self::set_from_y(model, w, y);
                true
            }
            _ => false,
        }
    }

    pub fn mouse_up(&mut self, model: &mut Model, ev: &Input) -> Out {
        let Some(p) = self.pressed.take() else { return Out::None };
        if self.hit(model, ev.x, ev.y) != Some(p) {
            return Out::Redraw;
        }
        match p {
            W::Close => model.exec(Command::ToggleEqWindow),
            W::Shade => model.exec(Command::ToggleEqShade),
            W::On => model.exec(Command::ToggleEq),
            W::Auto => model.exec(Command::ToggleEqAutoload),
            _ => {}
        }
        Out::Redraw
    }

    pub fn wheel(&mut self, model: &mut Model, x: i32, y: i32, steps: i32) {
        match self.hit(model, x, y) {
            Some(W::Preamp) => model.set_preamp(model.settings().preamp - steps * 2),
            Some(W::Band(i)) => model.set_eq_band(i, model.settings().eq_bands()[i] as i32 - steps * 2),
            _ => model.set_volume(model.settings().volume + steps * 8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linamp_skin::CursorKind as C;

    #[test]
    fn cursors_follow_the_control_under_the_mouse() {
        let m = Model::for_tests();
        let w = EqWin::new();
        assert_eq!(w.cursor(&m, 267, 5), C::EqClose);
        assert_eq!(w.cursor(&m, 25, 50), C::EqSlid);
        assert_eq!(w.cursor(&m, 82, 50), C::EqSlid);
        assert_eq!(w.cursor(&m, 100, 5), C::EqTitle);
        assert_eq!(w.cursor(&m, 5, 105), C::EqNormal);
    }
}
