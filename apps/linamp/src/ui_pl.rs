//! The playlist editor window.

use crate::font::Font;
use crate::menu::MenuItem;
use crate::model::Model;
use crate::render::{skin_text, sprite};
use crate::ui::{Input, Out};
use linamp_core::text::format_duration_ms;
use linamp_core::Command;
use linamp_skin::sprites::{pl as p, r, Rect};
use linamp_skin::CursorKind;
use linamp_skin::{Bitmap, Sheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum W {
    Title,
    Close,
    Shade,
    Row(usize),
    Scroll,
    MenuButton(usize),
    Mini(usize),
    Resize,
}

pub struct PlWin {
    pressed: Option<W>,
    /// First visible row.
    pub scroll: usize,
    /// Open bottom-bar menu and the item under the cursor.
    menu: Option<usize>,
    menu_hover: Option<usize>,
    /// The mouse went up over the menu button that opened it: keep it open.
    menu_sticky: bool,
    /// Row-drag reordering: the row the drag is currently at.
    drag_row: Option<usize>,
    anchor: usize,
    scroll_grab: i32,
}

/// Command for item `k` (bottom-up) of bottom-bar menu `m`.
type MenuBuilder = fn(&Model) -> Vec<MenuItem>;

fn menu_command(m: usize, k: usize) -> Option<Result<Command, MenuBuilder>> {
    use Command as C;
    Some(match (m, k) {
        (0, 0) => Ok(C::PlAddFiles),
        (0, 1) => Ok(C::PlAddFolder),
        (0, 2) => Ok(C::PlAddUrl),
        (1, 0) => Err(|_| {
            vec![
                MenuItem::cmd("Remove missing files", "Alt+Del", C::RemoveMissing),
                MenuItem::cmd("Physically remove selected file(s)…", "", C::PhysicallyRemove),
            ]
        }),
        (1, 1) => Ok(C::PlRemoveSelected),
        (1, 2) => Ok(C::PlCropSelected),
        (1, 3) => Ok(C::ClearPlaylist),
        (2, 0) => Ok(C::SelectAll),
        (2, 1) => Ok(C::SelectNone),
        (2, 2) => Ok(C::InvertSelection),
        (3, 0) => Err(|m: &Model| m.options_menu()),
        (3, 1) => Ok(C::FileInfo),
        (3, 2) => Err(|m: &Model| m.sort_menu()),
        (4, 0) => Ok(C::PlaylistOpen),
        (4, 1) => Ok(C::PlaylistSave),
        (4, 2) => Ok(C::ClearPlaylist),
        _ => return None,
    })
}

impl PlWin {
    pub fn new() -> Self {
        Self { pressed: None, scroll: 0, menu: None, menu_hover: None, menu_sticky: false, drag_row: None, anchor: 0, scroll_grab: 0 }
    }

    pub fn size(model: &Model) -> (i32, i32) {
        let s = model.settings();
        if s.pe_ws {
            (s.pe_width, 14)
        } else {
            (s.pe_width, s.pe_height)
        }
    }

    fn row_h(model: &Model) -> i32 {
        model.settings().pe_fontsize + 3
    }

    fn list_rect(w: i32, h: i32) -> Rect {
        r(12, 20, w - 32, h - 58)
    }

    fn visible_rows(model: &Model, h: i32) -> usize {
        (Self::list_rect(275, h).h / Self::row_h(model)).max(1) as usize
    }

    fn button_x(m: usize, w: i32) -> i32 {
        [14, 43, 72, 101, w - 44][m]
    }

    fn menu_item_rect(m: usize, k: usize, w: i32, h: i32) -> Rect {
        r(Self::button_x(m, w), h - 30 - k as i32 * 18, 22, 18)
    }

    fn clamp_scroll(&mut self, model: &Model, h: i32) {
        let max = model.playlist.len().saturating_sub(Self::visible_rows(model, h));
        self.scroll = self.scroll.min(max);
    }

    /// Scrolls so `row` is visible.
    pub fn ensure_visible(&mut self, model: &Model, row: usize) {
        let (_, h) = Self::size(model);
        let vis = Self::visible_rows(model, h);
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + vis {
            self.scroll = row + 1 - vis;
        }
    }

    /// The skin cursor for the control under the mouse (or being dragged).
    pub fn cursor(&self, model: &Model, x: i32, y: i32) -> CursorKind {
        use CursorKind as C;
        let shade = model.settings().pe_ws;
        match (shade, self.pressed.or_else(|| self.hit(model, x, y))) {
            (_, Some(W::Close)) => C::PClose,
            (_, Some(W::Shade)) => C::PWinBut,
            (false, Some(W::Resize)) => C::PSize,
            (true, Some(W::Resize)) => C::PWSSize,
            (true, _) => C::PWSNorm,
            (false, Some(W::Title)) => C::PTBar,
            (false, Some(W::Scroll)) => C::PVScroll,
            _ => C::PNormal,
        }
    }

    fn hit(&self, model: &Model, x: i32, y: i32) -> Option<W> {
        let (w, h) = Self::size(model);
        if model.settings().pe_ws {
            if r(w - 21, 3, 9, 9).contains(x, y) {
                return Some(W::Shade);
            }
            if r(w - 11, 3, 9, 9).contains(x, y) {
                return Some(W::Close);
            }
            return Some(W::Title);
        }
        if r(w - 20, 3, 9, 9).contains(x, y) {
            return Some(W::Shade);
        }
        if r(w - 11, 3, 9, 9).contains(x, y) {
            return Some(W::Close);
        }
        if y < 20 {
            return Some(W::Title);
        }
        let list = Self::list_rect(w, h);
        if list.contains(x, y) {
            let row = self.scroll + ((y - list.y) / Self::row_h(model)) as usize;
            return Some(W::Row(row));
        }
        if r(w - 15, 20, 8, h - 58).contains(x, y) {
            return Some(W::Scroll);
        }
        if r(w - 20, h - 20, 20, 20).contains(x, y) {
            return Some(W::Resize);
        }
        for m in 0..5 {
            let bw = if m == 4 { 22 } else { 25 };
            if r(Self::button_x(m, w), h - 30, bw, 18).contains(x, y) {
                return Some(W::MenuButton(m));
            }
        }
        let mini = [(6, 8), (14, 8), (22, 9), (31, 8), (39, 8), (48, 9)];
        for (i, (mx, mw)) in mini.iter().enumerate() {
            if r(w - 150 + mx, h - 16, *mw, 9).contains(x, y) {
                return Some(W::Mini(i));
            }
        }
        None
    }

    fn menu_hit(&self, model: &Model, x: i32, y: i32) -> Option<usize> {
        let m = self.menu?;
        let (w, h) = Self::size(model);
        (0..p::menu_items(m).len()).find(|&k| Self::menu_item_rect(m, k, w, h).contains(x, y))
    }

    pub fn draw(&mut self, model: &mut Model, font: &mut Font, b: &mut Bitmap, active: bool) {
        let (w, h) = Self::size(model);
        let active = active || !model.settings().dim_inactive;
        let sel = |pair: (Rect, Rect)| if active { pair.0 } else { pair.1 };
        let skin = &model.skin;
        if model.settings().pe_ws {
            sprite(b, skin, Sheet::PlEdit, sel(p::SHADE_LEFT), 0, 0);
            let mut x = 25;
            while x < w - 50 {
                sprite(b, skin, Sheet::PlEdit, p::SHADE_TILE, x, 0);
                x += 25;
            }
            sprite(b, skin, Sheet::PlEdit, sel(p::SHADE_RIGHT), w - 50, 0);
            if self.pressed == Some(W::Shade) {
                sprite(b, skin, Sheet::PlEdit, p::SHADE_PRESSED, w - 21, 3);
            }
            if self.pressed == Some(W::Close) {
                sprite(b, skin, Sheet::PlEdit, p::CLOSE_PRESSED, w - 11, 3);
            }
            let title = model.current_title();
            skin_text(b, skin, r(5, 4, w - 60, 6), &title, 0);
            return;
        }
        // Frame.
        sprite(b, skin, Sheet::PlEdit, sel(p::TOP_LEFT), 0, 0);
        let mut x = 25;
        while x < w - 25 {
            sprite(b, skin, Sheet::PlEdit, sel(p::TOP_TILE), x, 0);
            x += 25;
        }
        sprite(b, skin, Sheet::PlEdit, sel(p::TITLE), (w - 100) / 2, 0);
        sprite(b, skin, Sheet::PlEdit, sel(p::TOP_RIGHT), w - 25, 0);
        let mut y = 20;
        while y < h - 38 {
            sprite(b, skin, Sheet::PlEdit, p::LEFT_TILE, 0, y);
            sprite(b, skin, Sheet::PlEdit, p::RIGHT_TILE, w - 20, y);
            y += 29;
        }
        let mut x = 125;
        while x < w - 150 {
            sprite(b, skin, Sheet::PlEdit, p::BOTTOM_TILE, x, h - 38);
            x += 25;
        }
        sprite(b, skin, Sheet::PlEdit, p::BOTTOM_LEFT, 0, h - 38);
        if w >= 350 {
            sprite(b, skin, Sheet::PlEdit, p::VIS, w - 225, h - 38);
        }
        sprite(b, skin, Sheet::PlEdit, p::BOTTOM_RIGHT, w - 150, h - 38);
        if self.pressed == Some(W::Close) {
            sprite(b, skin, Sheet::PlEdit, p::CLOSE_PRESSED, w - 11, 3);
        }
        if self.pressed == Some(W::Shade) {
            sprite(b, skin, Sheet::PlEdit, p::SHADE_PRESSED, w - 20, 3);
        }
        // Track list.
        let list = Self::list_rect(w, h);
        let colors = skin.pl.clone();
        b.fill(list.x, list.y, list.w, list.h, colors.normal_bg);
        self.clamp_scroll(model, h);
        let row_h = Self::row_h(model);
        let px = model.settings().pe_fontsize as u32;
        let (shownums, zeropad) = (model.settings().shownumsinpl, model.settings().zeropadplnum);
        let total = model.playlist.len();
        let digits = total.max(1).to_string().len();
        let current = model.playlist.current();
        let rows = Self::visible_rows(model, h);
        for vi in 0..rows {
            let i = self.scroll + vi;
            let Some(e) = model.playlist.entry(i) else { break };
            let ry = list.y + vi as i32 * row_h;
            if e.selected {
                b.fill(list.x, ry, list.w, row_h, colors.selected_bg);
            }
            let color = if Some(i) == current { colors.current } else { colors.normal };
            let time = e.length_ms.map(format_duration_ms).unwrap_or_default();
            let tw = font.width(&time, px);
            let label = match (shownums, zeropad) {
                (true, true) => format!("{:0digits$}. {}", i + 1, e.display),
                (true, false) => format!("{}. {}", i + 1, e.display),
                _ => e.display.clone(),
            };
            let avail = list.w - tw - 8;
            let fitted = font.fit(&label, px, avail);
            font.draw(b, list.x + 2, ry + 1, &fitted, px, color, list.x, list.w);
            font.draw(b, list.x + list.w - tw - 2, ry + 1, &time, px, color, list.x, list.w);
            if i == model.playlist.cursor && active {
                // Focus rectangle (dotted).
                for xx in (list.x..list.x + list.w).step_by(2) {
                    b.set(xx, ry, color);
                    b.set(xx, ry + row_h - 1, color);
                }
            }
        }
        // Scroll thumb.
        let track = h - 58 - 18;
        let max_scroll = total.saturating_sub(rows);
        let ty = if max_scroll == 0 { 0 } else { (self.scroll as i32 * track) / max_scroll as i32 };
        let thumb = if self.pressed == Some(W::Scroll) { p::SCROLL_THUMB.pressed } else { p::SCROLL_THUMB.normal };
        sprite(b, skin, Sheet::PlEdit, thumb, w - 15, 20 + ty);
        // Running time and mini time.
        let (sel_ms, total_ms, unknown) = model.playlist.durations();
        let plus = if unknown { "+" } else { "" };
        let running = format!("{}/{}{}", format_duration_ms(sel_ms), format_duration_ms(total_ms), plus);
        skin_text(b, skin, r(w - 143, h - 28, 85, 6), &running, 0);
        let sh = model.engine.shared();
        if sh.state() != linamp_audio::PlayState::Stopped {
            let pos = sh.position_ms();
            let shown = match (model.settings().timeleftmode, sh.length_ms()) {
                (true, Some(l)) => format!("-{}", format_duration_ms(l.saturating_sub(pos))),
                _ => format_duration_ms(pos),
            };
            skin_text(b, skin, r(w - 82, h - 14, 30, 6), &format!("{shown:>6}"), 0);
        }
        // Visualizer extension when wide.
        if w >= 350 {
            model.vis.draw(b, w - 222, h - 28, 72, 18, model.settings(), &model.skin.viscolor);
        }
        // Bottom-bar menu.
        if let Some(m) = self.menu {
            let skin = &model.skin;
            let items = p::menu_items(m);
            for (k, (normal, pressed)) in items.iter().enumerate() {
                let rc = Self::menu_item_rect(m, k, w, h);
                let src = if self.menu_hover == Some(k) { *pressed } else { *normal };
                sprite(b, skin, Sheet::PlEdit, src, rc.x, rc.y);
            }
            let bar = p::MENU_BARS[m];
            let top = h - 30 - (items.len() as i32 - 1) * 18;
            sprite(b, skin, Sheet::PlEdit, bar, Self::button_x(m, w) - 3, top);
        }
    }

    pub fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
        self.menu_hover = None;
    }

    fn run_menu_item(&mut self, model: &mut Model, m: usize, k: usize) -> Out {
        self.close_menu();
        match menu_command(m, k) {
            Some(Ok(c)) => {
                model.exec(c);
                Out::Redraw
            }
            Some(Err(build)) => Out::Menu(build(model)),
            None => Out::Redraw,
        }
    }

    pub fn mouse_down(&mut self, model: &mut Model, ev: &Input) -> Out {
        if self.menu.is_some() {
            let m = self.menu.expect("checked");
            if let Some(k) = self.menu_hit(model, ev.x, ev.y) {
                return self.run_menu_item(model, m, k);
            }
            self.close_menu();
            return Out::Redraw;
        }
        let w = self.hit(model, ev.x, ev.y);
        if ev.right {
            if let Some(W::Row(i)) = w {
                if i < model.playlist.len() && !model.playlist.entry(i).is_some_and(|e| e.selected) {
                    model.playlist.select_only(i);
                }
                return Out::Menu(model.playlist_row_menu());
            }
            return Out::Menu(model.main_menu());
        }
        match w {
            None => Out::None,
            Some(W::Title) => {
                if ev.double {
                    model.exec(Command::TogglePlShade);
                    return Out::None;
                }
                Out::BeginMove
            }
            Some(W::Resize) => Out::BeginResize,
            Some(W::MenuButton(m)) => {
                self.menu = Some(m);
                self.menu_hover = None;
                self.menu_sticky = false;
                self.pressed = w;
                Out::Capture
            }
            Some(W::Row(i)) => {
                let len = model.playlist.len();
                if i >= len {
                    model.playlist.select_all(false);
                    return Out::Redraw;
                }
                if ev.double {
                    model.play_index(i);
                    return Out::Redraw;
                }
                if ev.shift {
                    model.playlist.select_range(self.anchor.min(len - 1), i);
                } else if ev.ctrl {
                    model.playlist.toggle_selected(i);
                    self.anchor = i;
                } else {
                    if !model.playlist.entry(i).is_some_and(|e| e.selected) {
                        model.playlist.select_only(i);
                    }
                    self.anchor = i;
                    self.drag_row = Some(i);
                }
                model.playlist.cursor = i;
                model.pl_dirty += 1;
                self.pressed = w;
                Out::Capture
            }
            Some(W::Scroll) => {
                self.pressed = w;
                self.scroll_grab = 9;
                self.scroll_to_y(model, ev.y);
                Out::Capture
            }
            Some(other) => {
                self.pressed = Some(other);
                Out::Capture
            }
        }
    }

    fn scroll_to_y(&mut self, model: &Model, y: i32) {
        let (_, h) = Self::size(model);
        let track = (h - 58 - 18).max(1);
        let rows = Self::visible_rows(model, h);
        let max = model.playlist.len().saturating_sub(rows);
        let frac = ((y - 20 - self.scroll_grab) as f32 / track as f32).clamp(0.0, 1.0);
        self.scroll = (frac * max as f32).round() as usize;
    }

    pub fn mouse_move(&mut self, model: &mut Model, x: i32, y: i32) -> bool {
        if self.menu.is_some() {
            let hover = self.menu_hit(model, x, y);
            if hover != self.menu_hover {
                self.menu_hover = hover;
                return true;
            }
            return false;
        }
        match self.pressed {
            Some(W::Scroll) => {
                self.scroll_to_y(model, y);
                true
            }
            Some(W::Row(_)) => {
                let Some(from) = self.drag_row else { return false };
                let (_, h) = Self::size(model);
                let list = Self::list_rect(275, h);
                let row = (self.scroll as i32 + (y - list.y).div_euclid(Self::row_h(model))).max(0) as usize;
                if row != from {
                    let moved = model.playlist.move_selected(row as isize - from as isize);
                    self.drag_row = Some((from as isize + moved) as usize);
                    model.pl_dirty += 1;
                    model.update_next();
                    self.ensure_visible(model, row.min(model.playlist.len().saturating_sub(1)));
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub fn mouse_up(&mut self, model: &mut Model, ev: &Input) -> Out {
        let pressed = self.pressed.take();
        self.drag_row = None;
        if let Some(m) = self.menu {
            if let Some(k) = self.menu_hit(model, ev.x, ev.y) {
                return self.run_menu_item(model, m, k);
            }
            if pressed == Some(W::MenuButton(m)) && self.hit(model, ev.x, ev.y) == pressed {
                self.menu_sticky = true;
                return Out::Redraw;
            }
            self.close_menu();
            return Out::Redraw;
        }
        let Some(pw) = pressed else { return Out::None };
        if self.hit(model, ev.x, ev.y) != Some(pw) {
            return Out::Redraw;
        }
        use Command as C;
        let cmd = match pw {
            W::Close => Some(C::TogglePlaylistWindow),
            W::Shade => Some(C::TogglePlShade),
            W::Mini(0) => Some(C::Previous),
            W::Mini(1) => Some(C::Play),
            W::Mini(2) => Some(C::Pause),
            W::Mini(3) => Some(C::Stop),
            W::Mini(4) => Some(C::Next),
            W::Mini(5) => Some(C::OpenFile),
            _ => None,
        };
        if let Some(c) = cmd {
            model.exec(c);
        }
        Out::Redraw
    }

    pub fn wheel(&mut self, model: &mut Model, steps: i32) {
        let (_, h) = Self::size(model);
        self.scroll = (self.scroll as i64 - steps as i64 * 3).max(0) as usize;
        self.clamp_scroll(model, h);
    }

    /// Keyboard navigation; returns true if handled.
    pub fn key_nav(&mut self, model: &mut Model, key: Nav, shift: bool) -> bool {
        let len = model.playlist.len();
        if len == 0 {
            return false;
        }
        let (_, h) = Self::size(model);
        let page = (Self::visible_rows(model, h) / 5).max(1);
        let cur = model.playlist.cursor.min(len - 1);
        let target = match key {
            Nav::Up => cur.saturating_sub(1),
            Nav::Down => (cur + 1).min(len - 1),
            Nav::PageUp => cur.saturating_sub(page),
            Nav::PageDown => (cur + page).min(len - 1),
            Nav::Home => 0,
            Nav::End => len - 1,
        };
        if shift {
            model.playlist.select_range(self.anchor.min(len - 1), target);
        } else {
            model.playlist.select_only(target);
            self.anchor = target;
        }
        model.playlist.cursor = target;
        self.ensure_visible(model, target);
        model.pl_dirty += 1;
        true
    }

    /// Resizes in 25x29 steps from a drag delta (logical pixels).
    pub fn resize_to(model: &mut Model, start: (i32, i32), dx: i32, dy: i32) -> bool {
        let snap = |v: i32, min: i32, step: i32| min + (((v - min) as f32 / step as f32).round() as i32).max(0) * step;
        let nw = snap(start.0 + dx, p::MIN_SIZE.0, p::STEP.0);
        let nh = snap(start.1 + dy, p::MIN_SIZE.1, p::STEP.1);
        let s = model.settings_mut();
        if (nw, nh) != (s.pe_width, s.pe_height) {
            s.pe_width = nw;
            s.pe_height = nh;
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

#[cfg(test)]
mod tests {
    use super::*;
    use linamp_skin::CursorKind as C;

    #[test]
    fn cursors_follow_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = PlWin::new();
        assert_eq!(w.cursor(&m, 267, 5), C::PClose);
        assert_eq!(w.cursor(&m, 257, 5), C::PWinBut);
        assert_eq!(w.cursor(&m, 100, 5), C::PTBar);
        assert_eq!(w.cursor(&m, 100, 50), C::PNormal);
        assert_eq!(w.cursor(&m, 262, 40), C::PVScroll);
        assert_eq!(w.cursor(&m, 268, 226), C::PSize);
        m.settings_mut().pe_ws = true;
        assert_eq!(w.cursor(&m, 100, 5), C::PWSNorm);
        assert_eq!(w.cursor(&m, 267, 5), C::PClose);
    }
}
