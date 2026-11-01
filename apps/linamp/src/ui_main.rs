//! The main window.

use crate::menu::MenuItem;
use crate::model::Model;
use crate::render::{skin_text, skin_text_width, sprite, time_digits};
use crate::ui::{Input, Out};
use linamp_audio::PlayState;
use linamp_core::text::format_duration_ms;
use linamp_core::Command;
use linamp_skin::sprites::{main as m, r, Rect};
use linamp_skin::CursorKind;
use linamp_skin::{Bitmap, Sheet};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum W {
    Options,
    Minimize,
    Shade,
    Close,
    Prev,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    Shuffle,
    Repeat,
    Eq,
    Pl,
    Seek,
    Volume,
    Balance,
    Clutter(usize),
    Marquee,
    Time,
    Vis,
    ShadeTransport(usize),
    ShadeSeek,
    TitleBar,
}

fn btn(at: (i32, i32), w: i32, h: i32) -> Rect {
    r(at.0, at.1, w, h)
}

pub struct MainWin {
    pressed: Option<W>,
    /// Pixel offset of the thumb under the cursor when a slider drag began.
    grab: i32,
    /// Seek preview (ms) while dragging the position bar.
    seek_preview: Option<u64>,
    marquee_offset: i32,
    marquee_drag: Option<(i32, i32)>,
    last_scroll: Instant,
}

impl MainWin {
    pub fn new() -> Self {
        Self { pressed: None, grab: 0, seek_preview: None, marquee_offset: 0, marquee_drag: None, last_scroll: Instant::now() }
    }

    pub fn size(model: &Model) -> (i32, i32) {
        if model.settings().windowshade {
            m::SHADE_SIZE
        } else {
            m::SIZE
        }
    }

    fn hit(&self, model: &Model, x: i32, y: i32) -> Option<W> {
        let shade = model.settings().windowshade;
        let checks: &[(Rect, W)] = &[
            (btn(m::AT_OPTIONS, 9, 9), W::Options),
            (btn(m::AT_MINIMIZE, 9, 9), W::Minimize),
            (btn(m::AT_SHADE, 9, 9), W::Shade),
            (btn(m::AT_CLOSE, 9, 9), W::Close),
        ];
        for (rect, w) in checks {
            if rect.contains(x, y) {
                return Some(*w);
            }
        }
        if shade {
            for (i, rect) in m::SHADE_TRANSPORT.iter().enumerate() {
                if rect.contains(x, y) {
                    return Some(W::ShadeTransport(i));
                }
            }
            if btn(m::AT_SHADE_POS, 17, 7).contains(x, y) {
                return Some(W::ShadeSeek);
            }
            return (y < 14).then_some(W::TitleBar);
        }
        let widgets: &[(Rect, W)] = &[
            (btn(m::AT_PREV, 23, 18), W::Prev),
            (btn(m::AT_PLAY, 23, 18), W::Play),
            (btn(m::AT_PAUSE, 23, 18), W::Pause),
            (btn(m::AT_STOP, 23, 18), W::Stop),
            (btn(m::AT_NEXT, 22, 18), W::Next),
            (btn(m::AT_EJECT, 22, 16), W::Eject),
            (btn(m::AT_SHUFFLE, 47, 15), W::Shuffle),
            (btn(m::AT_REPEAT, 28, 15), W::Repeat),
            (btn(m::AT_EQ, 23, 12), W::Eq),
            (btn(m::AT_PL, 23, 12), W::Pl),
            (m::POS_AREA, W::Seek),
            (m::VOLUME_AREA, W::Volume),
            (m::BALANCE_AREA, W::Balance),
            (r(m::MARQUEE.x - 2, 24, m::MARQUEE.w + 4, 12), W::Marquee),
            (m::TIME_AREA, W::Time),
            (m::VIS, W::Vis),
        ];
        for (rect, w) in widgets {
            if rect.contains(x, y) {
                if *w == W::Seek && model.engine.shared().length_ms().is_none() {
                    continue;
                }
                return Some(*w);
            }
        }
        let (cx, cy) = m::AT_CLUTTER;
        if x >= cx && x < cx + 8 && y >= cy && y < cy + 43 {
            let rel = y - cy;
            let idx = m::CLUTTER_OFFSETS.iter().rposition(|&o| rel >= o).unwrap_or(0);
            return Some(W::Clutter(idx));
        }
        (y < 14).then_some(W::TitleBar)
    }

    /// The skin cursor for the control under the mouse (or being dragged).
    pub fn cursor(&self, model: &Model, x: i32, y: i32) -> CursorKind {
        use CursorKind as C;
        let shade = model.settings().windowshade;
        match (shade, self.pressed.or_else(|| self.hit(model, x, y))) {
            (false, Some(W::Options)) => C::MainMenu,
            (true, Some(W::Options)) => C::MMenu,
            (_, Some(W::Minimize)) => C::Min,
            (_, Some(W::Close)) => C::Close,
            (_, Some(W::Shade)) => C::WinBut,
            (_, Some(W::Seek)) => C::PosBar,
            (_, Some(W::ShadeSeek)) => C::WSPosBar,
            (_, Some(W::Volume | W::Balance)) => C::VolBal,
            (_, Some(W::Marquee)) => C::SongName,
            (false, Some(W::TitleBar)) => C::TitleBar,
            (true, _) => C::WSNormal,
            _ => C::Normal,
        }
    }

    /// Tooltip for the control under the mouse: a few controls have their
    /// own text, as in the original; everywhere else shows the program name.
    pub fn tooltip(&self, model: &Model, x: i32, y: i32) -> String {
        let text = match self.hit(model, x, y) {
            Some(W::Options) => "Main Menu",
            Some(W::Minimize) => "Minimize",
            Some(W::Shade) => "Toggle Windowshade Mode",
            Some(W::Close) => "Close",
            Some(W::Clutter(0)) => "Options Menu",
            Some(W::Clutter(1)) => "Toggle Always-On-Top",
            Some(W::Clutter(2)) => "File Info Box",
            Some(W::Clutter(3)) => "Toggle Doublesize Mode",
            Some(W::Clutter(4)) => "Visualization Menu",
            Some(W::Marquee) => "Song Title (right-click for options)",
            _ => return format!("Linamp {}", env!("CARGO_PKG_VERSION")),
        };
        text.to_string()
    }

    fn marquee_text(&self, model: &Model) -> String {
        if let Some(ms) = self.seek_preview {
            let len = model.engine.shared().length_ms().unwrap_or(1).max(1);
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

    /// Advances the marquee; returns true if it moved.
    pub fn tick(&mut self, model: &Model) -> bool {
        if self.marquee_drag.is_some() || !model.settings().autoscrollname {
            return false;
        }
        let text = self.marquee_text(model);
        if skin_text_width(&text) <= m::MARQUEE.w {
            self.marquee_offset = 0;
            return false;
        }
        if self.last_scroll.elapsed() >= Duration::from_millis(180) {
            self.last_scroll = Instant::now();
            self.marquee_offset += 5;
            return true;
        }
        false
    }

    pub fn draw(&mut self, model: &mut Model, b: &mut Bitmap, active: bool) {
        let skin = &model.skin;
        let s = model.settings();
        let p = self.pressed;
        let pressed = |w: W| p == Some(w);
        let pick = |bt: linamp_skin::sprites::Button, w: W| if pressed(w) { bt.pressed } else { bt.normal };
        let sh = model.engine.shared();
        let state = sh.state();
        let pos = sh.position_ms();
        let len = sh.length_ms();
        if s.windowshade {
            sprite(b, skin, Sheet::TitleBar, if active || !s.dim_inactive { m::SHADE_ACTIVE } else { m::SHADE_INACTIVE }, 0, 0);
            sprite(b, skin, Sheet::TitleBar, pick(m::OPTIONS, W::Options), m::AT_OPTIONS.0, m::AT_OPTIONS.1);
            sprite(b, skin, Sheet::TitleBar, pick(m::MINIMIZE, W::Minimize), m::AT_MINIMIZE.0, m::AT_MINIMIZE.1);
            sprite(b, skin, Sheet::TitleBar, pick(m::UNSHADE, W::Shade), m::AT_SHADE.0, m::AT_SHADE.1);
            sprite(b, skin, Sheet::TitleBar, pick(m::CLOSE, W::Close), m::AT_CLOSE.0, m::AT_CLOSE.1);
            if state != PlayState::Stopped {
                let secs = pos / 1000;
                let t = format!("{:>2}:{:02}", (secs / 60).min(99), secs % 60);
                skin_text(b, skin, r(m::AT_SHADE_TIME.0, m::AT_SHADE_TIME.1, 25, 6), &t, 0);
                if let Some(len) = len.filter(|&l| l > 0) {
                    let (sx, sy) = m::AT_SHADE_POS;
                    sprite(b, skin, Sheet::TitleBar, m::SHADE_POS_BG, sx, sy);
                    let frac = self.seek_preview.unwrap_or(pos) as f32 / len as f32;
                    let tx = sx + (frac.clamp(0.0, 1.0) * 14.0) as i32;
                    sprite(b, skin, Sheet::TitleBar, m::SHADE_POS_THUMB, tx, sy);
                }
            }
            return;
        }
        sprite(b, skin, Sheet::Main, r(0, 0, 275, 116), 0, 0);
        sprite(b, skin, Sheet::TitleBar, if active || !s.dim_inactive { m::TITLE_ACTIVE } else { m::TITLE_INACTIVE }, 0, 0);
        sprite(b, skin, Sheet::TitleBar, pick(m::OPTIONS, W::Options), m::AT_OPTIONS.0, m::AT_OPTIONS.1);
        sprite(b, skin, Sheet::TitleBar, pick(m::MINIMIZE, W::Minimize), m::AT_MINIMIZE.0, m::AT_MINIMIZE.1);
        sprite(b, skin, Sheet::TitleBar, pick(m::SHADE, W::Shade), m::AT_SHADE.0, m::AT_SHADE.1);
        sprite(b, skin, Sheet::TitleBar, pick(m::CLOSE, W::Close), m::AT_CLOSE.0, m::AT_CLOSE.1);
        // Clutter bar: A and D light up when active.
        let (cx, cy) = m::AT_CLUTTER;
        sprite(b, skin, Sheet::TitleBar, m::CLUTTER_BG, cx, cy);
        for i in 0..5 {
            let lit = pressed(W::Clutter(i)) || (i == 1 && s.aot) || (i == 3 && s.dsize);
            if lit {
                sprite(b, skin, Sheet::TitleBar, m::CLUTTER_SELECTED[i], cx, cy + m::CLUTTER_OFFSETS[i]);
            }
        }
        // Status indicator.
        let (ix, iy) = m::AT_PLAYPAUS;
        let (wx, wy) = m::AT_WORK;
        match state {
            PlayState::Playing => {
                sprite(b, skin, Sheet::PlayPaus, m::PLAYING, ix, iy);
                sprite(b, skin, Sheet::PlayPaus, m::WORKING, wx, wy);
            }
            PlayState::Paused => sprite(b, skin, Sheet::PlayPaus, m::PAUSED, ix, iy),
            PlayState::Stopped => sprite(b, skin, Sheet::PlayPaus, m::STOPPED, ix, iy),
        }
        // Time.
        if state != PlayState::Stopped {
            // Paused time blinks once a second.
            let blink_off = state == PlayState::Paused
                && (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() / 500) % 2 == 1;
            let shown = self.seek_preview.unwrap_or(pos);
            let (secs, neg) = match (s.timeleftmode, len) {
                (true, Some(l)) => (l.saturating_sub(shown) / 1000, true),
                _ => (shown / 1000, false),
            };
            if !blink_off {
                time_digits(b, skin, Some(secs), neg, 0, 0);
            }
        }
        // Marquee, kbps, kHz, mono/stereo.
        let text = self.marquee_text(model);
        let skin = &model.skin;
        let tw = skin_text_width(&text);
        if tw > m::MARQUEE.w {
            let full = format!("{text}  ***  ");
            let period = skin_text_width(&full);
            let off = self.marquee_offset.rem_euclid(period);
            skin_text(b, skin, m::MARQUEE, &format!("{full}{full}"), off);
        } else {
            skin_text(b, skin, m::MARQUEE, &text, 0);
        }
        if state != PlayState::Stopped {
            let kbps = sh.bitrate_kbps();
            if kbps > 0 {
                skin_text(b, skin, m::KBPS, &format!("{:>3}", kbps.min(999)), 0);
            }
            let rate = sh.sample_rate();
            if rate > 0 {
                skin_text(b, skin, m::KHZ, &format!("{:>2}", (rate / 1000).min(99)), 0);
            }
            let stereo = sh.channels() >= 2;
            sprite(b, skin, Sheet::MonoSter, if stereo { m::STEREO_ON } else { m::STEREO_OFF }, m::AT_STEREO.0, m::AT_STEREO.1);
            sprite(b, skin, Sheet::MonoSter, if stereo { m::MONO_OFF } else { m::MONO_ON }, m::AT_MONO.0, m::AT_MONO.1);
        } else {
            sprite(b, skin, Sheet::MonoSter, m::STEREO_OFF, m::AT_STEREO.0, m::AT_STEREO.1);
            sprite(b, skin, Sheet::MonoSter, m::MONO_OFF, m::AT_MONO.0, m::AT_MONO.1);
        }
        // Visualizer.
        let vis_area = m::VIS;
        model.vis.draw(b, vis_area.x, vis_area.y, vis_area.w, vis_area.h, model.settings(), &model.skin.viscolor);
        let skin = &model.skin;
        let s = model.settings();
        // Position bar.
        if let Some(len) = len.filter(|&l| l > 0 && state != PlayState::Stopped) {
            let a = m::POS_AREA;
            sprite(b, skin, Sheet::PosBar, m::POS_BG, a.x, a.y);
            let range = a.w - m::POS_THUMB.normal.w;
            let frac = self.seek_preview.unwrap_or(pos) as f32 / len as f32;
            let tx = a.x + (frac.clamp(0.0, 1.0) * range as f32) as i32;
            sprite(b, skin, Sheet::PosBar, pick(m::POS_THUMB, W::Seek), tx, a.y);
        }
        // Volume and balance.
        let va = m::VOLUME_AREA;
        let vframe = (s.volume * 27 + 127) / 255;
        sprite(b, skin, Sheet::Volume, r(0, vframe * m::SLIDER_FRAME_STEP, 68, 13), va.x, va.y);
        if skin.sheet(Sheet::Volume).h > 422 {
            let tx = va.x + s.volume * (va.w - 14) / 255;
            sprite(b, skin, Sheet::Volume, pick(m::SLIDER_THUMB, W::Volume), tx, va.y + 1);
        }
        let ba = m::BALANCE_AREA;
        let bframe = (s.balance.abs() * 27 + 63) / 127;
        sprite(b, skin, Sheet::Balance, r(m::BALANCE_SRC_X, bframe * m::SLIDER_FRAME_STEP, 38, 13), ba.x, ba.y);
        if skin.sheet(Sheet::Balance).h > 422 {
            let tx = ba.x + (s.balance + 127) * (ba.w - 14) / 254;
            sprite(b, skin, Sheet::Balance, pick(m::SLIDER_THUMB, W::Balance), tx, ba.y + 1);
        }
        // Transport and toggles.
        for (bt, at, w) in [
            (m::PREV, m::AT_PREV, W::Prev),
            (m::PLAY, m::AT_PLAY, W::Play),
            (m::PAUSE, m::AT_PAUSE, W::Pause),
            (m::STOP, m::AT_STOP, W::Stop),
            (m::NEXT, m::AT_NEXT, W::Next),
            (m::EJECT, m::AT_EJECT, W::Eject),
        ] {
            sprite(b, skin, Sheet::CButtons, pick(bt, w), at.0, at.1);
        }
        for (t, at, w, on) in [
            (m::SHUFFLE, m::AT_SHUFFLE, W::Shuffle, s.shuffle),
            (m::REPEAT, m::AT_REPEAT, W::Repeat, s.repeat),
            (m::EQ, m::AT_EQ, W::Eq, s.eq_open),
            (m::PL, m::AT_PL, W::Pl, s.pe_open),
        ] {
            sprite(b, skin, Sheet::ShufRep, t.pick(on, pressed(w)), at.0, at.1);
        }
    }

    fn seek_ms_at(&self, model: &Model, x: i32, shade: bool) -> Option<u64> {
        let len = model.engine.shared().length_ms()?;
        let frac = if shade {
            (x - m::AT_SHADE_POS.0 - 1) as f32 / 14.0
        } else {
            let a = m::POS_AREA;
            (x - self.grab - a.x) as f32 / (a.w - m::POS_THUMB.normal.w) as f32
        };
        Some((frac.clamp(0.0, 1.0) * len as f32) as u64)
    }

    fn volume_at(&self, x: i32) -> i32 {
        let a = m::VOLUME_AREA;
        ((x - self.grab - a.x) * 255 / (a.w - 14)).clamp(0, 255)
    }

    fn balance_at(&self, x: i32) -> i32 {
        let a = m::BALANCE_AREA;
        let b = ((x - self.grab - a.x) * 254 / (a.w - 14) - 127).clamp(-127, 127);
        // Snap to centre.
        if b.abs() < 16 {
            0
        } else {
            b
        }
    }

    fn context_menu(&self, model: &Model, w: Option<W>) -> Vec<MenuItem> {
        use Command as C;
        match w {
            Some(W::Prev) => vec![
                MenuItem::cmd("Previous", "Click", C::Previous),
                MenuItem::cmd("Start of list", "Ctrl+Click", C::StartOfList),
                MenuItem::cmd("Rewind 5 seconds", "Shift+Click", C::Back5s),
            ],
            Some(W::Play) => vec![
                MenuItem::cmd("Play/restart", "Click", C::Play),
                MenuItem::cmd("Open URL…", "Ctrl+Click", C::OpenUrl),
                MenuItem::cmd("Open file…", "Shift+Click", C::OpenFile),
            ],
            Some(W::Pause) => vec![MenuItem::cmd("Pause/Unpause", "Click", C::Pause)],
            Some(W::Stop) => vec![
                MenuItem::cmd("Stop", "Click", C::Stop),
                MenuItem::cmd("Stop w/ fadeout", "Shift+Click", C::StopFadeout),
                MenuItem::cmd("Stop after current", "Ctrl+Click", C::StopAfterCurrent).checked(model.stop_after_current),
            ],
            Some(W::Next) => vec![
                MenuItem::cmd("Next", "Click", C::Next),
                MenuItem::cmd("Fast-forward 5 seconds", "Shift+Click", C::Fwd5s),
                MenuItem::cmd("End of list", "Ctrl+Click", C::EndOfList),
            ],
            Some(W::Eject) => vec![
                MenuItem::cmd("Open file…", "Click", C::OpenFile),
                MenuItem::cmd("Open folder…", "Shift+Click", C::OpenFolder),
                MenuItem::cmd("Open URL…", "Ctrl+Click", C::OpenUrl),
            ],
            Some(W::Seek) => vec![
                MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
                MenuItem::cmd("Rewind 5 seconds", "Left", C::Back5s),
                MenuItem::cmd("Forward 5 seconds", "Right", C::Fwd5s),
            ],
            Some(W::Repeat) => vec![
                MenuItem::cmd("Repeat", "R", C::ToggleRepeat).checked(model.settings().repeat),
                MenuItem::cmd("Manual playlist advance", "Shift+R", C::ToggleManualAdvance).checked(!model.settings().pladv),
            ],
            Some(W::Shuffle) => vec![MenuItem::cmd("Shuffle", "S", C::ToggleShuffle).checked(model.settings().shuffle)],
            Some(W::Time) => vec![
                MenuItem::cmd("Time elapsed", "Ctrl+T", C::TimeElapsed).checked(!model.settings().timeleftmode),
                MenuItem::cmd("Time remaining", "Ctrl+T", C::TimeRemaining).checked(model.settings().timeleftmode),
            ],
            Some(W::Marquee) => vec![
                MenuItem::cmd("View file info…", "Alt+3", C::FileInfo),
                MenuItem::cmd("Jump to file…", "J", C::JumpToFile),
                MenuItem::cmd("Jump to time…", "Ctrl+J", C::JumpToTime),
                MenuItem::cmd("Autoscroll song name", "", C::ToggleAutoscroll).checked(model.settings().autoscrollname),
            ],
            Some(W::Vis) | Some(W::Clutter(4)) => model.vis_menu(),
            _ => model.main_menu(),
        }
    }

    pub fn mouse_down(&mut self, model: &mut Model, ev: &Input) -> Out {
        let w = self.hit(model, ev.x, ev.y);
        if ev.right {
            return Out::Menu(self.context_menu(model, w));
        }
        let shade = model.settings().windowshade;
        match w {
            None | Some(W::TitleBar) => {
                if ev.double && ev.y < 14 {
                    model.exec(Command::ToggleWindowshade);
                    return Out::None;
                }
                if w == Some(W::TitleBar) || model.settings().easymove {
                    return Out::BeginMove;
                }
                Out::None
            }
            Some(W::Options) => Out::Menu(model.main_menu()),
            Some(W::Clutter(0)) => Out::Menu(model.options_menu()),
            Some(W::Vis) => {
                model.exec(Command::CycleVisMode);
                Out::Redraw
            }
            Some(W::Time) => {
                model.exec(Command::ToggleTimeMode);
                Out::Redraw
            }
            Some(W::Marquee) => {
                if ev.double {
                    model.exec(Command::FileInfo);
                    return Out::None;
                }
                self.marquee_drag = Some((ev.x, self.marquee_offset));
                self.pressed = w;
                Out::Capture
            }
            Some(W::Seek) => {
                let a = m::POS_AREA;
                let len = model.engine.shared().length_ms().unwrap_or(0).max(1);
                let pos = model.engine.shared().position_ms();
                let thumb_x = a.x + (pos as f32 / len as f32 * (a.w - 29) as f32) as i32;
                self.grab = if ev.x >= thumb_x && ev.x < thumb_x + 29 { ev.x - thumb_x } else { 14 };
                self.seek_preview = self.seek_ms_at(model, ev.x, false);
                self.pressed = w;
                Out::Capture
            }
            Some(W::ShadeSeek) => {
                self.seek_preview = self.seek_ms_at(model, ev.x, true);
                self.pressed = w;
                Out::Capture
            }
            Some(W::Volume) => {
                let thumb_x = m::VOLUME_AREA.x + model.settings().volume * (m::VOLUME_AREA.w - 14) / 255;
                self.grab = if ev.x >= thumb_x && ev.x < thumb_x + 14 { ev.x - thumb_x } else { 7 };
                let v = self.volume_at(ev.x);
                model.set_volume(v);
                self.pressed = w;
                Out::Capture
            }
            Some(W::Balance) => {
                let thumb_x = m::BALANCE_AREA.x + (model.settings().balance + 127) * (m::BALANCE_AREA.w - 14) / 254;
                self.grab = if ev.x >= thumb_x && ev.x < thumb_x + 14 { ev.x - thumb_x } else { 7 };
                let b = self.balance_at(ev.x);
                model.set_balance(b);
                self.pressed = w;
                Out::Capture
            }
            Some(other) => {
                let _ = shade;
                self.pressed = Some(other);
                Out::Capture
            }
        }
    }

    pub fn mouse_move(&mut self, model: &mut Model, x: i32, _y: i32) -> bool {
        match self.pressed {
            Some(W::Seek) => self.seek_preview = self.seek_ms_at(model, x, false),
            Some(W::ShadeSeek) => self.seek_preview = self.seek_ms_at(model, x, true),
            Some(W::Volume) => {
                let v = self.volume_at(x);
                if v != model.settings().volume {
                    model.set_volume(v);
                }
            }
            Some(W::Balance) => {
                let b = self.balance_at(x);
                if b != model.settings().balance {
                    model.set_balance(b);
                }
            }
            Some(W::Marquee) => {
                if let Some((sx, so)) = self.marquee_drag {
                    self.marquee_offset = so - (x - sx);
                }
            }
            _ => return false,
        }
        true
    }

    pub fn mouse_up(&mut self, model: &mut Model, ev: &Input) -> Out {
        let Some(pressed) = self.pressed.take() else { return Out::None };
        let over = self.hit(model, ev.x, ev.y) == Some(pressed);
        match pressed {
            W::Seek | W::ShadeSeek => {
                if let Some(ms) = self.seek_preview.take() {
                    model.seek_to(ms);
                }
                return Out::Redraw;
            }
            W::Volume | W::Balance => return Out::Redraw,
            W::Marquee => {
                self.marquee_drag = None;
                self.last_scroll = Instant::now() + Duration::from_secs(1);
                return Out::Redraw;
            }
            _ => {}
        }
        if !over {
            return Out::Redraw;
        }
        use Command as C;
        let cmd = match pressed {
            W::Minimize => Some(C::Minimize),
            W::Shade => Some(C::ToggleWindowshade),
            W::Close => Some(C::Exit),
            W::Prev | W::ShadeTransport(0) => Some(if ev.ctrl {
                C::StartOfList
            } else if ev.shift {
                C::Back5s
            } else {
                C::Previous
            }),
            W::Play | W::ShadeTransport(1) => Some(if ev.ctrl {
                C::OpenUrl
            } else if ev.shift {
                C::OpenFile
            } else {
                C::Play
            }),
            W::Pause | W::ShadeTransport(2) => Some(C::Pause),
            W::Stop | W::ShadeTransport(3) => Some(if ev.ctrl {
                C::StopAfterCurrent
            } else if ev.shift {
                C::StopFadeout
            } else {
                C::Stop
            }),
            W::Next | W::ShadeTransport(4) => Some(if ev.ctrl {
                C::EndOfList
            } else if ev.shift {
                C::Fwd5s
            } else {
                C::Next
            }),
            W::Eject | W::ShadeTransport(5) => Some(if ev.ctrl {
                C::OpenUrl
            } else if ev.shift {
                C::OpenFolder
            } else {
                C::OpenFile
            }),
            W::Shuffle => Some(C::ToggleShuffle),
            W::Repeat => Some(C::ToggleRepeat),
            W::Eq => Some(C::ToggleEqWindow),
            W::Pl => Some(C::TogglePlaylistWindow),
            W::Clutter(1) => Some(C::ToggleAlwaysOnTop),
            W::Clutter(2) => Some(C::FileInfo),
            W::Clutter(3) => Some(C::ToggleDoubleSize),
            W::Clutter(4) => return Out::Menu(model.vis_menu()),
            _ => None,
        };
        if let Some(c) = cmd {
            model.exec(c);
        }
        Out::Redraw
    }

    pub fn wheel(&mut self, model: &mut Model, steps: i32) {
        model.set_volume(model.settings().volume + steps * 8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use linamp_skin::CursorKind as C;

    #[test]
    fn cursors_follow_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = MainWin::new();
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
    fn tooltips_name_the_control_under_the_mouse() {
        let mut m = Model::for_tests();
        let w = MainWin::new();
        assert_eq!(w.tooltip(&m, 13, 36), "Toggle Always-On-Top");
        assert_eq!(w.tooltip(&m, 180, 27), "Song Title (right-click for options)");
        assert_eq!(w.tooltip(&m, 9, 5), "Main Menu");
        // Elsewhere, the program name.
        assert!(w.tooltip(&m, 50, 97).starts_with("Linamp "));
        // The clutter bar is hidden in windowshade mode.
        m.settings_mut().windowshade = true;
        assert!(w.tooltip(&m, 13, 36).starts_with("Linamp "));
    }
}
