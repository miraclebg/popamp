//! Windowing: winit event loop, skinned windows, docking/snapping, pop-up
//! menus, keyboard shortcuts and drag & drop.

use crate::dialogs::{self, DialogResult};
use crate::font::Font;
use crate::menu::{self, Layout, MenuItem};
use crate::model::{AddMode, AppEvent, Model, UiReq, WinKind};
use crate::platform::{self, Platform, WindowRole};
use crate::render::present_scaled;
use crate::ui::{Input, Out};
use crate::ui_eq::EqWin;
use crate::ui_main::MainWin;
use crate::ui_pl::{Nav, PlWin};
use linamp_audio::PlayState;
use linamp_core::Command;
use linamp_skin::Bitmap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowId, WindowLevel};

const KINDS: [WinKind; 3] = [WinKind::Main, WinKind::Eq, WinKind::Pl];

fn idx(k: WinKind) -> usize {
    match k {
        WinKind::Main => 0,
        WinKind::Eq => 1,
        WinKind::Pl => 2,
    }
}

type Surface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

struct SkinWindow {
    window: Arc<Window>,
    surface: Surface,
    /// Top-left in physical screen pixels.
    pos: (i32, i32),
    /// Logical (1x) size.
    size: (i32, i32),
    visible: bool,
    focused: bool,
    /// Cursor in physical window coordinates.
    cursor: (i32, i32),
    frame: Bitmap,
    shape_sig: Option<(usize, i32, i32, i32)>,
}

struct Popup {
    window: Arc<Window>,
    surface: Surface,
    items: Vec<MenuItem>,
    layout: Layout,
    hover: Option<usize>,
    pos: (i32, i32),
    frame: Bitmap,
}

/// Tooltip timing, as in the original: appears after the mouse rests on a
/// control for a moment, disappears after a while.
const TIP_DELAY: Duration = Duration::from_millis(600);
const TIP_TIMEOUT: Duration = Duration::from_secs(7);

/// The tooltip for what the mouse rests on in the main window.
struct Tooltip {
    text: String,
    since: Instant,
    /// The window, once shown.
    shown: Option<(Arc<Window>, Surface)>,
}

enum Drag {
    Move { group: Vec<(WinKind, (i32, i32))>, start: (i32, i32) },
    Resize { start_size: (i32, i32), start: (i32, i32) },
}

pub struct App {
    pub model: Model,
    font: Font,
    mainw: MainWin,
    eqw: EqWin,
    plw: PlWin,
    wins: [Option<SkinWindow>; 3],
    popups: Vec<Popup>,
    tip: Option<Tooltip>,
    drag: Option<Drag>,
    capture: Option<WinKind>,
    mods: ModifiersState,
    last_click: Option<(WinKind, Instant, (i32, i32))>,
    drops: Vec<(WinKind, String)>,
    drop_time: Instant,
    seen_pl_dirty: u64,
    next_frame: Instant,
    last_slow: Instant,
    start_minimized: bool,
    popup_focus_check: Option<Instant>,
    /// `LINAMP_SNAPSHOT=<dir>`: write each window as PNG after a few frames and exit.
    snapshot: Option<(std::path::PathBuf, u32)>,
    platform: Platform,
    mpris: Option<crate::mpris::Mpris>,
}

impl App {
    pub fn new(model: Model, start_minimized: bool, mpris: Option<crate::mpris::Mpris>) -> App {
        let font = Font::load(&model.skin.pl.font);
        App {
            model,
            font,
            mainw: MainWin::new(),
            eqw: EqWin::new(),
            plw: PlWin::new(),
            wins: [None, None, None],
            popups: Vec::new(),
            tip: None,
            drag: None,
            capture: None,
            mods: ModifiersState::empty(),
            last_click: None,
            drops: Vec::new(),
            drop_time: Instant::now(),
            seen_pl_dirty: u64::MAX,
            next_frame: Instant::now(),
            last_slow: Instant::now(),
            start_minimized,
            popup_focus_check: None,
            snapshot: std::env::var_os("LINAMP_SNAPSHOT").map(|d| (std::path::PathBuf::from(d), 0)),
            platform: Platform::connect(),
            mpris,
        }
    }

    fn scale(&self) -> i32 {
        if self.model.settings().dsize {
            2
        } else {
            1
        }
    }

    fn logical_size(&self, k: WinKind) -> (i32, i32) {
        match k {
            WinKind::Main => MainWin::size(&self.model),
            WinKind::Eq => EqWin::size(&self.model),
            WinKind::Pl => PlWin::size(&self.model),
        }
    }

    fn wanted_visible(&self, k: WinKind) -> bool {
        let s = self.model.settings();
        match k {
            WinKind::Main => true,
            WinKind::Eq => s.eq_open,
            WinKind::Pl => s.pe_open,
        }
    }

    fn saved_pos(&self, k: WinKind) -> (i32, i32) {
        let s = self.model.settings();
        match k {
            WinKind::Main => (s.wx, s.wy),
            WinKind::Eq => (s.eq_wx, s.eq_wy),
            WinKind::Pl => (s.pe_wx, s.pe_wy),
        }
    }

    fn store_pos(&mut self, k: WinKind, p: (i32, i32)) {
        let s = self.model.settings_mut();
        match k {
            WinKind::Main => (s.wx, s.wy) = p,
            WinKind::Eq => (s.eq_wx, s.eq_wy) = p,
            WinKind::Pl => (s.pe_wx, s.pe_wy) = p,
        }
    }

    fn win(&self, k: WinKind) -> Option<&SkinWindow> {
        self.wins[idx(k)].as_ref()
    }

    fn win_mut(&mut self, k: WinKind) -> Option<&mut SkinWindow> {
        self.wins[idx(k)].as_mut()
    }

    fn kind_of(&self, id: WindowId) -> Option<WinKind> {
        KINDS.into_iter().find(|&k| self.win(k).is_some_and(|w| w.window.id() == id))
    }

    fn create_windows(&mut self, el: &ActiveEventLoop) {
        let scale = self.scale();
        // Keep windows on a connected monitor.
        let monitors: Vec<(i32, i32, i32, i32)> =
            el.available_monitors().map(|m| (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32)).collect();
        for k in KINDS {
            let (w, h) = self.logical_size(k);
            let mut pos = self.saved_pos(k);
            if self.model.settings().keeponscreen && !monitors.is_empty() {
                let on_screen = monitors
                    .iter()
                    .any(|&(mx, my, mw, mh)| pos.0 + 20 > mx && pos.0 < mx + mw - 20 && pos.1 >= my - 5 && pos.1 < my + mh - 14);
                if !on_screen {
                    let (mx, my, _, _) = monitors[0];
                    pos = (mx + 100, my + 100 + idx(k) as i32 * 116 * scale);
                }
            }
            let title = match k {
                WinKind::Main => "Linamp",
                WinKind::Eq => "Linamp Equalizer",
                WinKind::Pl => "Linamp Playlist Editor",
            };
            let role = if k == WinKind::Main { WindowRole::Main } else { WindowRole::Docked };
            let mut attrs = platform::attributes(role)
                .with_title(title)
                .with_inner_size(PhysicalSize::new((w * scale) as u32, (h * scale) as u32))
                .with_position(PhysicalPosition::new(pos.0, pos.1))
                .with_visible(false)
                .with_window_icon(window_icon());
            if self.model.settings().aot {
                attrs = attrs.with_window_level(WindowLevel::AlwaysOnTop);
            }
            let window = match el.create_window(attrs) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    log::error!("cannot create window: {e}");
                    el.exit();
                    return;
                }
            };
            let context = match softbuffer::Context::new(window.clone()) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("softbuffer: {e}");
                    el.exit();
                    return;
                }
            };
            let surface = match Surface::new(&context, window.clone()) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("softbuffer surface: {e}");
                    el.exit();
                    return;
                }
            };
            window.set_cursor(winit::window::CursorIcon::Default);
            self.wins[idx(k)] = Some(SkinWindow {
                window,
                surface,
                pos,
                size: (w, h),
                visible: false,
                focused: false,
                cursor: (0, 0),
                frame: Bitmap::new(w, h, 0),
                shape_sig: None,
            });
            self.store_pos(k, pos);
        }
        self.apply_layout();
        if self.start_minimized {
            for k in KINDS {
                if let Some(w) = self.win(k) {
                    w.window.set_minimized(true);
                }
            }
        }
    }

    /// Applies sizes, scale and visibility; keeps docked windows attached
    /// when a window above them changes height.
    fn apply_layout(&mut self) {
        let scale = self.scale();
        type Geometry = (i32, i32, i32, i32, bool);
        let old: Vec<Option<Geometry>> = KINDS
            .iter()
            .map(|&k| {
                self.win(k).map(|w| {
                    let s = w.window.inner_size();
                    (w.pos.0, w.pos.1, s.width as i32, s.height as i32, w.visible)
                })
            })
            .collect();
        let mut new_pos: Vec<(i32, i32)> = KINDS.iter().map(|&k| self.win(k).map(|w| w.pos).unwrap_or((0, 0))).collect();
        for (i, &k) in KINDS.iter().enumerate() {
            let Some((x, y, ow, oh, vis)) = old[i] else { continue };
            let (lw, lh) = self.logical_size(k);
            let (nw, nh) = (lw * scale, lh * scale);
            if !vis || (nw == ow && nh == oh) {
                continue;
            }
            // Shift windows docked below/right of this one.
            for (j, &other) in KINDS.iter().enumerate() {
                if j == i {
                    continue;
                }
                let Some((ox, oy, oww, ohh, ovis)) = old[j] else { continue };
                if !ovis {
                    continue;
                }
                let h_overlap = ox < x + ow && ox + oww > x;
                let v_overlap = oy < y + oh && oy + ohh > y;
                if (oy - (y + oh)).abs() <= 1 && h_overlap {
                    new_pos[j].1 += nh - oh;
                } else if (ox - (x + ow)).abs() <= 1 && v_overlap {
                    new_pos[j].0 += nw - ow;
                }
                let _ = other;
            }
        }
        for (i, &k) in KINDS.iter().enumerate() {
            let visible = self.wanted_visible(k);
            let (lw, lh) = self.logical_size(k);
            let pos = new_pos[i];
            if let Some(w) = self.win_mut(k) {
                let _ = w.window.request_inner_size(PhysicalSize::new((lw * scale) as u32, (lh * scale) as u32));
                if w.pos != pos {
                    w.window.set_outer_position(PhysicalPosition::new(pos.0, pos.1));
                    w.pos = pos;
                }
                w.size = (lw, lh);
                if w.visible != visible {
                    w.window.set_visible(visible);
                    w.visible = visible;
                }
                w.window.request_redraw();
            }
            self.store_pos(k, pos);
        }
        self.update_shapes();
    }

    /// Applies `region.txt` window shapes.
    fn update_shapes(&mut self) {
        let scale = self.scale();
        let regions = &self.model.skin.regions;
        let s = &self.model.config.settings;
        let skin_ptr = &self.model.skin as *const _ as usize;
        for k in [WinKind::Main, WinKind::Eq] {
            let polys = match k {
                WinKind::Main if s.windowshade => regions.window_shade.as_ref(),
                WinKind::Main => regions.normal.as_ref(),
                _ if s.eq_ws => regions.equalizer_ws.as_ref(),
                _ => regions.equalizer.as_ref(),
            };
            let Some(w) = self.wins[idx(k)].as_mut() else { continue };
            let sig = (skin_ptr ^ polys.map(|p| p.len()).unwrap_or(0), w.size.0, w.size.1, scale);
            if w.shape_sig == Some(sig) {
                continue;
            }
            w.shape_sig = Some(sig);
            if let Err(e) = self.platform.set_shape(&w.window, polys.map(|v| v.as_slice()), w.size, scale) {
                log::debug!("shape: {e}");
            }
        }
    }

    fn render(&mut self, k: WinKind) {
        let scale = self.scale() as u32;
        let i = idx(k);
        let Some(w) = self.wins[i].as_mut() else { return };
        if !w.visible {
            return;
        }
        let (lw, lh) = w.size;
        if w.frame.w != lw || w.frame.h != lh {
            w.frame = Bitmap::new(lw, lh, 0);
        }
        let active = w.focused;
        match k {
            WinKind::Main => self.mainw.draw(&mut self.model, &mut w.frame, active),
            WinKind::Eq => self.eqw.draw(&mut self.model, &mut w.frame, active),
            WinKind::Pl => self.plw.draw(&mut self.model, &mut self.font, &mut w.frame, active),
        }
        let (pw, ph) = (lw as u32 * scale, lh as u32 * scale);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(pw), NonZeroU32::new(ph)) else { return };
        if w.surface.resize(nw, nh).is_err() {
            return;
        }
        if let Ok(mut buf) = w.surface.buffer_mut() {
            present_scaled(&w.frame, scale, &mut buf, pw);
            if let Err(e) = buf.present() {
                log::debug!("present failed: {e}");
            }
        }
    }

    fn redraw_all(&mut self) {
        for k in KINDS {
            self.render(k);
        }
        for i in 0..self.popups.len() {
            self.render_popup(i);
        }
    }

    // --- pop-up menus ---------------------------------------------------------

    fn cursor_screen(&self, k: WinKind) -> (i32, i32) {
        self.win(k).map(|w| (w.pos.0 + w.cursor.0, w.pos.1 + w.cursor.1)).unwrap_or((100, 100))
    }

    fn open_popup(&mut self, el: &ActiveEventLoop, items: Vec<MenuItem>, at: (i32, i32)) {
        if items.is_empty() {
            return;
        }
        let layout = menu::layout(&items, &mut self.font);
        let (mut x, mut y) = at;
        // Keep the menu on the monitor.
        if let Some(m) = self.win(WinKind::Main).and_then(|w| w.window.current_monitor()) {
            let (mx, my) = (m.position().x, m.position().y);
            let (mw, mh) = (m.size().width as i32, m.size().height as i32);
            if x + layout.w > mx + mw {
                x = (mx + mw - layout.w).max(mx);
            }
            if y + layout.h > my + mh {
                y = (my + mh - layout.h).max(my);
            }
        }
        let attrs = platform::attributes(WindowRole::Popup)
            .with_inner_size(PhysicalSize::new(layout.w as u32, layout.h as u32))
            .with_position(PhysicalPosition::new(x, y));
        let Ok(window) = el.create_window(attrs).map(Arc::new) else { return };
        let Ok(ctx) = softbuffer::Context::new(window.clone()) else { return };
        let Ok(surface) = Surface::new(&ctx, window.clone()) else { return };
        let frame = Bitmap::new(layout.w, layout.h, 0);
        self.popups.push(Popup { window, surface, items, layout, hover: None, pos: (x, y), frame });
        let i = self.popups.len() - 1;
        self.render_popup(i);
    }

    fn render_popup(&mut self, i: usize) {
        let Some(p) = self.popups.get_mut(i) else { return };
        menu::draw(&mut p.frame, &p.items, &p.layout, p.hover, &mut self.font);
        let (w, h) = (p.layout.w as u32, p.layout.h as u32);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else { return };
        if p.surface.resize(nw, nh).is_ok() {
            if let Ok(mut buf) = p.surface.buffer_mut() {
                present_scaled(&p.frame, 1, &mut buf, w);
                let _ = buf.present();
            }
        }
    }

    /// Starts the tooltip timer when the mouse rests on a new control.
    fn hover_tooltip(&mut self, k: WinKind, x: i32, y: i32) {
        if k != WinKind::Main || self.capture.is_some() {
            self.tip = None;
            return;
        }
        let text = self.mainw.tooltip(&self.model, x, y);
        if self.tip.as_ref().is_none_or(|t| t.text != text) {
            self.tip = Some(Tooltip { text, since: Instant::now(), shown: None });
        }
    }

    /// Shows the tooltip once the mouse has rested long enough, and hides it
    /// after a while or when a menu or drag takes over.
    fn update_tooltip(&mut self, el: &ActiveEventLoop) {
        if !self.popups.is_empty() || self.drag.is_some() {
            self.tip = None;
        }
        let Some(tip) = &mut self.tip else { return };
        let age = tip.since.elapsed();
        if age >= TIP_TIMEOUT {
            tip.shown = None;
            return;
        }
        if age < TIP_DELAY || tip.shown.is_some() {
            return;
        }
        let text = tip.text.clone();
        let frame = menu::tooltip(&text, &mut self.font);
        let (x, y) = self.cursor_screen(WinKind::Main);
        let attrs = platform::attributes(WindowRole::Tooltip)
            .with_title(text)
            .with_inner_size(PhysicalSize::new(frame.w as u32, frame.h as u32))
            .with_position(PhysicalPosition::new(x, y + 20))
            .with_window_level(WindowLevel::AlwaysOnTop);
        let Ok(window) = el.create_window(attrs).map(Arc::new) else { return };
        let Ok(ctx) = softbuffer::Context::new(window.clone()) else { return };
        let Ok(surface) = Surface::new(&ctx, window.clone()) else { return };
        window.request_redraw();
        if let Some(tip) = &mut self.tip {
            tip.shown = Some((window, surface));
        }
    }

    fn render_tooltip(&mut self) {
        let Some(Tooltip { text, shown: Some((_, surface)), .. }) = &mut self.tip else { return };
        let frame = menu::tooltip(text, &mut self.font);
        let (w, h) = (frame.w as u32, frame.h as u32);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else { return };
        if surface.resize(nw, nh).is_ok() {
            if let Ok(mut buf) = surface.buffer_mut() {
                present_scaled(&frame, 1, &mut buf, w);
                let _ = buf.present();
            }
        }
    }

    fn close_popups_from(&mut self, level: usize) {
        self.popups.truncate(level);
    }

    fn popup_hover(&mut self, el: &ActiveEventLoop, level: usize, hover: Option<usize>) {
        let Some(p) = self.popups.get_mut(level) else { return };
        if p.hover == hover {
            return;
        }
        p.hover = hover;
        let sub = hover.and_then(|h| {
            (!p.items[h].sub.is_empty()).then(|| (p.items[h].sub.clone(), p.pos.0 + p.layout.w - 3, p.pos.1 + p.layout.rows[h].0 - 3))
        });
        self.close_popups_from(level + 1);
        self.render_popup(level);
        if let Some((items, x, y)) = sub {
            self.open_popup(el, items, (x, y));
        }
    }

    fn popup_activate(&mut self, el: &ActiveEventLoop, level: usize, item: usize) {
        let Some(it) = self.popups.get(level).and_then(|p| p.items.get(item)).cloned() else { return };
        if !it.sub.is_empty() {
            self.popups[level].hover = None;
            self.popup_hover(el, level, Some(item));
            return;
        }
        if let Some(action) = it.action {
            self.popups.clear();
            self.model.on_menu(action);
            self.after_model(el);
        }
    }

    fn popup_key(&mut self, el: &ActiveEventLoop, code: KeyCode) {
        let level = self.popups.len() - 1;
        let p = &self.popups[level];
        let n = p.items.len();
        let enabled: Vec<usize> = (0..n).filter(|&i| p.items[i].enabled()).collect();
        let cur = p.hover.and_then(|h| enabled.iter().position(|&e| e == h));
        match code {
            KeyCode::Escape => self.popups.clear(),
            KeyCode::ArrowDown | KeyCode::ArrowUp if !enabled.is_empty() => {
                let next = match (cur, code) {
                    (None, KeyCode::ArrowDown) => 0,
                    (None, _) => enabled.len() - 1,
                    (Some(c), KeyCode::ArrowDown) => (c + 1) % enabled.len(),
                    (Some(c), _) => (c + enabled.len() - 1) % enabled.len(),
                };
                let item = enabled[next];
                self.popups[level].hover = Some(item);
                self.render_popup(level);
            }
            KeyCode::ArrowRight | KeyCode::Enter | KeyCode::Space => {
                if let Some(h) = self.popups[level].hover {
                    self.popup_activate(el, level, h);
                }
            }
            KeyCode::ArrowLeft if level > 0 => {
                self.popups.truncate(level);
            }
            _ => {}
        }
    }

    // --- docking & moving -------------------------------------------------------

    fn rect(&self, k: WinKind) -> Option<(i32, i32, i32, i32)> {
        let w = self.win(k)?;
        if !w.visible {
            return None;
        }
        let s = self.scale();
        Some((w.pos.0, w.pos.1, w.size.0 * s, w.size.1 * s))
    }

    fn docked_group(&self, root: WinKind) -> Vec<WinKind> {
        let mut group = vec![root];
        let touching = |a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)| {
            let h_overlap = a.0 < b.0 + b.2 && b.0 < a.0 + a.2;
            let v_overlap = a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
            (h_overlap && ((a.1 + a.3 - b.1).abs() <= 1 || (b.1 + b.3 - a.1).abs() <= 1))
                || (v_overlap && ((a.0 + a.2 - b.0).abs() <= 1 || (b.0 + b.2 - a.0).abs() <= 1))
        };
        let mut changed = true;
        while changed {
            changed = false;
            for k in KINDS {
                if group.contains(&k) {
                    continue;
                }
                let Some(r) = self.rect(k) else { continue };
                if group.iter().filter_map(|&g| self.rect(g)).any(|g| touching(g, r)) {
                    group.push(k);
                    changed = true;
                }
            }
        }
        group
    }

    fn begin_move(&mut self, k: WinKind) {
        let group = if k == WinKind::Main { self.docked_group(k) } else { vec![k] };
        let start = self.cursor_screen(k);
        let group = group.into_iter().filter_map(|g| self.win(g).map(|w| (g, w.pos))).collect();
        self.drag = Some(Drag::Move { group, start });
    }

    fn snap_delta(&self, group: &[(WinKind, (i32, i32))], dx: i32, dy: i32) -> (i32, i32) {
        let s = self.model.settings();
        if !s.snap {
            return (dx, dy);
        }
        let snap = s.snaplen;
        let scale = self.scale();
        let in_group = |k: WinKind| group.iter().any(|(g, _)| *g == k);
        let mut targets: Vec<(i32, i32, i32, i32)> = KINDS.iter().filter(|&&k| !in_group(k)).filter_map(|&k| self.rect(k)).collect();
        if let Some(m) = self.win(WinKind::Main).and_then(|w| w.window.current_monitor()) {
            let (mx, my, mw, mh) = (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32);
            // Screen edges as zero-thickness rectangles around the monitor.
            targets.push((mx - 10_000, my - 10_000, 10_000 + mx + mw + 10_000 - mx, 10_000));
            targets.push((mx - 10_000, my + mh, 30_000, 10_000));
            targets.push((mx - 10_000, my - 10_000, 10_000, 30_000));
            targets.push((mx + mw, my - 10_000, 10_000, 30_000));
        }
        let (mut best_x, mut best_y) = (None::<i32>, None::<i32>);
        for (k, start) in group {
            let Some(w) = self.win(*k) else { continue };
            if !w.visible {
                continue;
            }
            let (gw, gh) = (w.size.0 * scale, w.size.1 * scale);
            let (gx, gy) = (start.0 + dx, start.1 + dy);
            for &(tx, ty, tw, th) in &targets {
                let v_near = gy < ty + th + snap && gy + gh > ty - snap;
                let h_near = gx < tx + tw + snap && gx + gw > tx - snap;
                if v_near {
                    for cand in [tx + tw - gx, tx - (gx + gw), tx - gx, tx + tw - (gx + gw)] {
                        if cand.abs() <= snap && best_x.is_none_or(|b| cand.abs() < b.abs()) {
                            best_x = Some(cand);
                        }
                    }
                }
                if h_near {
                    for cand in [ty + th - gy, ty - (gy + gh), ty - gy, ty + th - (gy + gh)] {
                        if cand.abs() <= snap && best_y.is_none_or(|b| cand.abs() < b.abs()) {
                            best_y = Some(cand);
                        }
                    }
                }
            }
        }
        (dx + best_x.unwrap_or(0), dy + best_y.unwrap_or(0))
    }

    fn drag_to(&mut self, k: WinKind) {
        let cursor = self.cursor_screen(k);
        match &self.drag {
            Some(Drag::Move { group, start }) => {
                let (dx, dy) = (cursor.0 - start.0, cursor.1 - start.1);
                let group = group.clone();
                let (dx, dy) = self.snap_delta(&group, dx, dy);
                for (g, origin) in group {
                    let p = (origin.0 + dx, origin.1 + dy);
                    if let Some(w) = self.win_mut(g) {
                        if w.pos != p {
                            w.pos = p;
                            w.window.set_outer_position(PhysicalPosition::new(p.0, p.1));
                        }
                    }
                    self.store_pos(g, p);
                }
            }
            Some(Drag::Resize { start_size, start }) => {
                let s = self.scale();
                let (dx, dy) = ((cursor.0 - start.0) / s, (cursor.1 - start.1) / s);
                if PlWin::resize_to(&mut self.model, *start_size, dx, dy) {
                    self.apply_layout();
                }
            }
            None => {}
        }
    }

    // --- input -----------------------------------------------------------------

    fn input(&self, k: WinKind, right: bool, double: bool) -> Input {
        let s = self.scale();
        let c = self.win(k).map(|w| w.cursor).unwrap_or((0, 0));
        Input { x: c.0.div_euclid(s), y: c.1.div_euclid(s), right, shift: self.mods.shift_key(), ctrl: self.mods.control_key(), double }
    }

    fn handle_out(&mut self, el: &ActiveEventLoop, k: WinKind, out: Out) {
        match out {
            Out::None => {}
            Out::Redraw => self.render(k),
            Out::Capture => {
                self.capture = Some(k);
                self.render(k);
            }
            Out::BeginMove => self.begin_move(k),
            Out::BeginResize => {
                let start = self.cursor_screen(k);
                let s = self.model.settings();
                self.drag = Some(Drag::Resize { start_size: (s.pe_width, s.pe_height), start });
            }
            Out::Menu(items) => {
                let at = self.cursor_screen(k);
                self.capture = None;
                self.open_popup(el, items, at);
                self.render(k);
            }
        }
        self.after_model(el);
    }

    fn mouse_down(&mut self, el: &ActiveEventLoop, k: WinKind, button: MouseButton) {
        self.tip = None;
        if !self.popups.is_empty() {
            self.popups.clear();
            return;
        }
        if button != MouseButton::Left && button != MouseButton::Right {
            return;
        }
        let right = button == MouseButton::Right;
        let pos = self.win(k).map(|w| w.cursor).unwrap_or_default();
        let double = !right
            && self.last_click.is_some_and(|(lk, t, p)| {
                lk == k && t.elapsed() < Duration::from_millis(400) && (p.0 - pos.0).abs() < 4 && (p.1 - pos.1).abs() < 4
            });
        self.last_click = if right || double { None } else { Some((k, Instant::now(), pos)) };
        let input = self.input(k, right, double);
        let out = match k {
            WinKind::Main => self.mainw.mouse_down(&mut self.model, &input),
            WinKind::Eq => self.eqw.mouse_down(&mut self.model, &input),
            WinKind::Pl => self.plw.mouse_down(&mut self.model, &input),
        };
        self.handle_out(el, k, out);
    }

    fn mouse_up(&mut self, el: &ActiveEventLoop, k: WinKind) {
        if let Some(drag) = self.drag.take() {
            if let Drag::Move { group, .. } = drag {
                for (g, _) in group {
                    if let Some(p) = self.win(g).map(|w| w.pos) {
                        self.store_pos(g, p);
                    }
                }
            }
            return;
        }
        let target = self.capture.take().unwrap_or(k);
        let input = self.input(target, false, false);
        let out = match target {
            WinKind::Main => self.mainw.mouse_up(&mut self.model, &input),
            WinKind::Eq => self.eqw.mouse_up(&mut self.model, &input),
            WinKind::Pl => self.plw.mouse_up(&mut self.model, &input),
        };
        self.handle_out(el, target, out);
    }

    fn mouse_moved(&mut self, k: WinKind) {
        if self.drag.is_some() {
            self.drag_to(k);
            return;
        }
        let target = self.capture.unwrap_or(k);
        let input = self.input(target, false, false);
        self.hover_tooltip(target, input.x, input.y);
        let changed = match target {
            WinKind::Main => self.mainw.mouse_move(&mut self.model, input.x, input.y),
            WinKind::Eq => self.eqw.mouse_move(&mut self.model, input.x, input.y),
            WinKind::Pl => self.plw.mouse_move(&mut self.model, input.x, input.y),
        };
        if changed {
            self.render(target);
            if target == WinKind::Eq {
                self.render(WinKind::Main);
            }
        }
    }

    fn key(&mut self, el: &ActiveEventLoop, k: WinKind, ev: &KeyEvent) {
        if ev.state != ElementState::Pressed {
            return;
        }
        let PhysicalKey::Code(code) = ev.physical_key else { return };
        if !self.popups.is_empty() {
            self.popup_key(el, code);
            return;
        }
        if k == WinKind::Pl && self.plw.menu_open() && code == KeyCode::Escape {
            self.plw.close_menu();
            self.render(k);
            return;
        }
        let (ctrl, shift, alt) = (self.mods.control_key(), self.mods.shift_key(), self.mods.alt_key());
        use Command as C;
        use KeyCode as K;
        // Window-specific keys first.
        if k == WinKind::Eq && !ctrl && !alt {
            const UP: [K; 10] =
                [K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9, K::Digit0];
            const DOWN: [K; 10] = [K::KeyQ, K::KeyW, K::KeyE, K::KeyR, K::KeyT, K::KeyY, K::KeyU, K::KeyI, K::KeyO, K::KeyP];
            if let Some(b) = UP.iter().position(|&c| c == code) {
                let v = self.model.settings().eq_bands()[b] as i32 - 1;
                self.model.set_eq_band(b, v);
                return self.redraw_after(el, k);
            }
            if let Some(b) = DOWN.iter().position(|&c| c == code) {
                let v = self.model.settings().eq_bands()[b] as i32 + 1;
                self.model.set_eq_band(b, v);
                return self.redraw_after(el, k);
            }
            match code {
                K::Backquote => {
                    let v = self.model.settings().preamp - 1;
                    self.model.set_preamp(v);
                    return self.redraw_after(el, k);
                }
                K::Tab => {
                    let v = self.model.settings().preamp + 1;
                    self.model.set_preamp(v);
                    return self.redraw_after(el, k);
                }
                K::KeyN => return self.run(el, C::ToggleEq),
                K::KeyA => return self.run(el, C::ToggleEqAutoload),
                K::KeyS => {
                    let items = self.model.eq_presets_menu();
                    let at = self.cursor_screen(k);
                    self.open_popup(el, items, at);
                    return;
                }
                _ => {}
            }
        }
        if k == WinKind::Eq && ctrl && alt && code == K::KeyS {
            return self.run(el, C::EqPresetLoad);
        }
        if k == WinKind::Pl {
            let nav = match code {
                K::ArrowUp if !alt => Some(Nav::Up),
                K::ArrowDown if !alt => Some(Nav::Down),
                K::PageUp => Some(Nav::PageUp),
                K::PageDown => Some(Nav::PageDown),
                K::Home => Some(Nav::Home),
                K::End => Some(Nav::End),
                _ => None,
            };
            if let Some(n) = nav {
                self.plw.key_nav(&mut self.model, n, shift);
                return self.redraw_after(el, k);
            }
            let cmd = match code {
                K::ArrowUp if alt => Some(C::MoveSelectionUp),
                K::ArrowDown if alt => Some(C::MoveSelectionDown),
                K::Enter | K::NumpadEnter => Some(C::PlaySelected),
                K::Delete if ctrl && shift => Some(C::ClearPlaylist),
                K::Delete if ctrl => Some(C::PlCropSelected),
                K::Delete if alt => Some(C::RemoveMissing),
                K::Delete => Some(C::PlRemoveSelected),
                K::KeyA if ctrl && alt => Some(C::ToggleAlwaysOnTop),
                K::KeyA if ctrl => Some(C::SelectAll),
                K::KeyI if ctrl => Some(C::InvertSelection),
                K::KeyN if ctrl => Some(C::ClearPlaylist),
                K::KeyO if ctrl => Some(C::PlaylistOpen),
                K::KeyS if ctrl => Some(C::PlaylistSave),
                K::KeyE if ctrl => Some(C::EditPlaylistEntry),
                K::KeyW if ctrl => Some(C::TogglePlShade),
                K::KeyR if ctrl && shift => Some(C::RandomizeList),
                K::KeyR if ctrl => Some(C::ReverseList),
                K::Digit1 if ctrl && shift => Some(C::SortByTitle),
                K::Digit2 if ctrl && shift => Some(C::SortByFileName),
                K::Digit3 if ctrl && shift => Some(C::SortByPath),
                K::KeyL if ctrl => Some(C::PlAddUrl),
                K::KeyL if shift => Some(C::PlAddFolder),
                K::KeyL if !alt => Some(C::PlAddFiles),
                K::Insert => Some(C::PlAddFolder),
                _ => None,
            };
            if let Some(c) = cmd {
                return self.run(el, c);
            }
        }
        if k == WinKind::Eq && ctrl && code == K::KeyW {
            return self.run(el, C::ToggleEqShade);
        }
        let cmd = match code {
            K::F1 if ctrl => Some(C::About),
            K::F1 => Some(C::About),
            K::KeyA if ctrl => Some(C::ToggleAlwaysOnTop),
            K::KeyW if ctrl => Some(C::ToggleWindowshade),
            K::KeyD if ctrl => Some(C::ToggleDoubleSize),
            K::KeyE if ctrl => Some(C::ToggleEasyMove),
            K::KeyT if ctrl => Some(C::ToggleTimeMode),
            K::KeyJ if ctrl => Some(C::JumpToTime),
            K::KeyP if ctrl => Some(C::Preferences),
            K::KeyK if ctrl && shift => Some(C::ToggleVis),
            K::KeyK if ctrl || alt => Some(C::SelectVisPlugin),
            K::KeyZ if ctrl => Some(C::StartOfList),
            K::KeyB if ctrl => Some(C::EndOfList),
            K::KeyV if ctrl => Some(C::StopAfterCurrent),
            K::KeyL if ctrl => Some(C::OpenUrl),
            K::KeyW if alt => Some(C::ToggleMainWindow),
            K::KeyE if alt => Some(C::TogglePlaylistWindow),
            K::KeyG if alt => Some(C::ToggleEqWindow),
            K::KeyS if alt => Some(C::SkinBrowser),
            K::KeyM if alt => Some(C::Minimize),
            K::Digit3 if alt => Some(C::FileInfo),
            K::KeyF if alt => {
                let items = self.model.main_menu();
                let at = self.cursor_screen(k);
                self.open_popup(el, items, at);
                return;
            }
            K::Tab if ctrl => {
                self.cycle_focus(k);
                return;
            }
            _ if ctrl || alt => None,
            K::KeyZ | K::Numpad4 => Some(C::Previous),
            K::KeyX | K::Numpad5 => Some(C::Play),
            K::KeyC => Some(C::Pause),
            K::KeyV if shift => Some(C::StopFadeout),
            K::KeyV => Some(C::Stop),
            K::KeyB | K::Numpad6 => Some(C::Next),
            K::KeyR if shift => Some(C::ToggleManualAdvance),
            K::KeyR => Some(C::ToggleRepeat),
            K::KeyS => Some(C::ToggleShuffle),
            K::KeyJ | K::NumpadDecimal => Some(C::JumpToFile),
            K::KeyL if shift => Some(C::OpenFolder),
            K::KeyL | K::Numpad0 => Some(C::OpenFile),
            K::Insert => Some(C::OpenFolder),
            K::ArrowLeft | K::Numpad7 => Some(C::Back5s),
            K::ArrowRight | K::Numpad9 => Some(C::Fwd5s),
            K::ArrowUp | K::Numpad8 => Some(C::VolumeUp),
            K::ArrowDown | K::Numpad2 => Some(C::VolumeDown),
            K::Numpad1 => Some(C::TenTracksBack),
            K::Numpad3 => Some(C::TenTracksForward),
            _ => None,
        };
        if let Some(c) = cmd {
            self.run(el, c);
        }
    }

    fn run(&mut self, el: &ActiveEventLoop, c: Command) {
        self.model.exec(c);
        self.after_model(el);
        self.redraw_all();
    }

    fn redraw_after(&mut self, el: &ActiveEventLoop, k: WinKind) {
        self.after_model(el);
        self.render(k);
        self.render(WinKind::Main);
    }

    fn cycle_focus(&mut self, from: WinKind) {
        let start = idx(from);
        for i in 1..=3 {
            let k = KINDS[(start + i) % 3];
            if let Some(w) = self.win(k).filter(|w| w.visible) {
                w.window.focus_window();
                return;
            }
        }
    }

    /// Executes window-system requests queued by the model.
    fn after_model(&mut self, el: &ActiveEventLoop) {
        let reqs: Vec<UiReq> = std::mem::take(&mut self.model.ui);
        for r in reqs {
            match r {
                UiReq::Dialog(kind, req) => {
                    let events = self.model.events().clone();
                    let _ = std::thread::Builder::new().name("linamp-dialog".into()).spawn(move || {
                        let values = dialogs::execute(&req);
                        events.send(AppEvent::Dialog(DialogResult { kind, values }));
                    });
                }
                UiReq::Menu(items) => {
                    let at = self.cursor_screen(WinKind::Main);
                    self.open_popup(el, items, at);
                }
                UiReq::Layout => {
                    self.apply_layout();
                    self.redraw_all();
                }
                UiReq::Minimize => {
                    for k in KINDS {
                        if let Some(w) = self.win(k).filter(|w| w.visible) {
                            w.window.set_minimized(true);
                        }
                    }
                }
                UiReq::Raise => {
                    for k in KINDS {
                        if let Some(w) = self.win(k).filter(|w| w.visible) {
                            w.window.set_minimized(false);
                        }
                    }
                    if let Some(w) = self.win(WinKind::Main) {
                        w.window.focus_window();
                    }
                }
                UiReq::Quit => {
                    self.model.save_session();
                    el.exit();
                }
                UiReq::SkinChanged => {
                    self.font = Font::load(&self.model.skin.pl.font);
                    for k in KINDS {
                        if let Some(w) = self.win_mut(k) {
                            w.shape_sig = None;
                        }
                    }
                    self.update_shapes();
                    self.redraw_all();
                }
                UiReq::AlwaysOnTop => {
                    let level = if self.model.settings().aot { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal };
                    for k in KINDS {
                        if let Some(w) = self.win(k) {
                            w.window.set_window_level(level);
                        }
                    }
                    self.render(WinKind::Main);
                }
            }
        }
        if let Some(m) = &mut self.mpris {
            m.update(self.model.mpris_snapshot());
        }
        if self.model.pl_dirty != self.seen_pl_dirty {
            self.seen_pl_dirty = self.model.pl_dirty;
            if let Some(cur) = self.model.playlist.current() {
                if self.model.status.playing_id.is_some() {
                    let _ = cur;
                }
            }
            self.render(WinKind::Pl);
        }
    }

    fn flush_drops(&mut self, el: &ActiveEventLoop) {
        if self.drops.is_empty() || self.drop_time.elapsed() < Duration::from_millis(120) {
            return;
        }
        let drops = std::mem::take(&mut self.drops);
        let k = drops[0].0;
        let files: Vec<String> = drops.into_iter().map(|(_, f)| f).collect();
        match k {
            WinKind::Pl => {
                // Insert at the row under the cursor.
                let input = self.input(WinKind::Pl, false, false);
                let row_h = self.model.settings().pe_fontsize + 3;
                let at = self.plw.scroll + ((input.y - 20).max(0) / row_h) as usize;
                self.model.add(files, AddMode::Insert(at.min(self.model.playlist.len())));
            }
            _ if self.mods.shift_key() => self.model.add(files, AddMode::Append),
            _ => self.model.add(files, AddMode::ReplaceAndPlay),
        }
        self.after_model(el);
    }

    /// Per-frame work: vis analysis, marquee, time display.
    fn tick(&mut self) {
        let sh = self.model.engine.shared().clone();
        let state = sh.state();
        if state == PlayState::Playing {
            let played = sh.played_frames();
            sh.vis.lock().read(played, self.model.vis.samples_mut());
        } else {
            self.model.vis.silence();
        }
        let rate = sh.sample_rate();
        let settings = self.model.settings().clone();
        self.model.vis.analyze(&settings, rate);
        self.mainw.tick(&self.model);
        self.render(WinKind::Main);
        if self.last_slow.elapsed() >= Duration::from_millis(500) {
            self.last_slow = Instant::now();
            if state != PlayState::Stopped || settings.pe_width >= 350 {
                self.render(WinKind::Pl);
            }
        } else if settings.pe_width >= 350 && state == PlayState::Playing {
            self.render(WinKind::Pl);
        }
    }
}

/// The application logo as the window icon (taskbar, alt-tab).
fn window_icon() -> Option<winit::window::Icon> {
    static PNG: &[u8] = include_bytes!("../../../assets/logo/png/linamp-64.png");
    let img = image::load_from_memory(PNG).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    winit::window::Icon::from_rgba(img.into_raw(), w, h).ok()
}

impl ApplicationHandler<AppEvent> for App {
    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. } | StartCause::Poll) || Instant::now() >= self.next_frame {
            let s = self.model.settings();
            let fps = if self.model.state() == PlayState::Playing && s.sa != 0 { s.vis_fps.max(1) } else { 6 };
            self.next_frame = Instant::now() + Duration::from_millis(1000 / fps as u64);
            self.tick();
            self.update_tooltip(el);
            self.flush_drops(el);
            if let Some((dir, frames)) = &mut self.snapshot {
                *frames += 1;
                if *frames == 8 {
                    let dir = dir.clone();
                    let _ = std::fs::create_dir_all(&dir);
                    for k in KINDS {
                        if let Some(w) = self.win(k).filter(|w| w.visible) {
                            let name = format!("{k:?}.png").to_lowercase();
                            if let Err(e) = w.frame.save_png(&dir.join(name)) {
                                log::error!("snapshot: {e}");
                            }
                        }
                    }
                    self.model.save_session();
                    el.exit();
                }
            }
            if let Some(t) = self.popup_focus_check {
                if t.elapsed() > Duration::from_millis(150) {
                    self.popup_focus_check = None;
                    let any_focused = KINDS.iter().any(|&k| self.win(k).is_some_and(|w| w.focused));
                    if !any_focused {
                        self.popups.clear();
                    }
                }
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }

    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.wins[0].is_none() {
            self.create_windows(el);
            self.after_model(el);
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: AppEvent) {
        match ev {
            AppEvent::Engine(e) => {
                self.model.on_engine(e);
                if let Some(cur) = self.model.playlist.current() {
                    self.plw.ensure_visible(&self.model, cur);
                }
                self.render(WinKind::Main);
                self.render(WinKind::Pl);
            }
            AppEvent::Meta(id, m) => self.model.on_meta(id, m),
            AppEvent::Expanded { items, mode } => self.model.on_expanded(items, mode),
            AppEvent::Dialog(r) => self.model.on_dialog(r),
            AppEvent::Remote(req) => {
                let reply = self.model.on_remote(req.remotes);
                let _ = req.reply.send(reply);
            }
        }
        self.after_model(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        if let Some(level) = self.popups.iter().position(|p| p.window.id() == id) {
            match ev {
                WindowEvent::CursorMoved { position, .. } => {
                    let p = &self.popups[level];
                    let h = menu::hit(&p.layout, &p.items, position.x as i32, position.y as i32);
                    if h.is_some() {
                        self.popup_hover(el, level, h);
                    }
                }
                WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left | MouseButton::Right, .. } => {
                    if let Some(h) = self.popups[level].hover {
                        self.popup_activate(el, level, h);
                    }
                }
                WindowEvent::RedrawRequested => self.render_popup(level),
                _ => {}
            }
            return;
        }
        if self.tip.as_ref().and_then(|t| t.shown.as_ref()).is_some_and(|(w, _)| w.id() == id) {
            if let WindowEvent::RedrawRequested = ev {
                self.render_tooltip();
            }
            return;
        }
        let Some(k) = self.kind_of(id) else { return };
        match ev {
            WindowEvent::CursorLeft { .. } => self.tip = None,
            WindowEvent::CloseRequested => self.run(el, Command::Exit),
            WindowEvent::RedrawRequested => self.render(k),
            WindowEvent::Focused(f) => {
                if let Some(w) = self.win_mut(k) {
                    w.focused = f;
                }
                if !f && !self.popups.is_empty() {
                    self.popup_focus_check = Some(Instant::now());
                }
                if !f && k == WinKind::Pl && self.plw.menu_open() {
                    self.plw.close_menu();
                }
                self.render(k);
            }
            WindowEvent::Moved(p) => {
                // The WM may adjust positions; trust it when not dragging.
                if self.drag.is_none() {
                    if let Some(w) = self.win_mut(k) {
                        w.pos = (p.x, p.y);
                    }
                    self.store_pos(k, (p.x, p.y));
                }
            }
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(w) = self.win_mut(k) {
                    w.cursor = (position.x as i32, position.y as i32);
                }
                self.mouse_moved(k);
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => self.mouse_down(el, k, button),
                ElementState::Released if button == MouseButton::Left => self.mouse_up(el, k),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 20.0).round() as i32,
                };
                if steps != 0 {
                    let input = self.input(k, false, false);
                    match k {
                        WinKind::Main => self.mainw.wheel(&mut self.model, steps),
                        WinKind::Eq => self.eqw.wheel(&mut self.model, input.x, input.y, steps),
                        WinKind::Pl => self.plw.wheel(&mut self.model, steps),
                    }
                    self.render(k);
                    self.render(WinKind::Main);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(el, k, &event),
            WindowEvent::DroppedFile(path) => {
                self.drops.push((k, path.to_string_lossy().into_owned()));
                self.drop_time = Instant::now();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        if !self.model.quit {
            self.model.save_session();
        }
    }
}
