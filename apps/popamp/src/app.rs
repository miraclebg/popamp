//! Windowing: winit event loop, skinned windows, docking/snapping, pop-up
//! menus, keyboard shortcuts and drag & drop.

use crate::canvas::Canvas;
use crate::dialogs::{self, DialogResult};
use crate::font::Fonts;
use crate::menu::{self, Layout, MenuItem};
use crate::model::{AddMode, AppEvent, Model, UiReq};
use crate::panels::{Panel, PanelKind};
use crate::platform::{self, Mask, Platform, WindowRole};
use crate::scale::Scale;
use crate::skinwin::{Nav, SkinWin};
use crate::ui::{Input, Out};
use crate::widgets::{self, Event};
use popamp_audio::PlayState;
use popamp_core::Command;
use popamp_skin::layout::{Paint, Placement, Shape, WindowKey};
use popamp_skin::textcfg::region_mask;
use popamp_skin::CursorKind;
use popamp_skin::{Bitmap, Picture, Skin};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{CursorIcon, CustomCursor, Window, WindowId, WindowLevel};

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
    shape_sig: Option<(usize, i32, i32)>,
    /// The skin cursor last set on this window.
    cursor_kind: Option<CursorKind>,
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

/// One of Popamp's own dialogs (see `panels.rs`) in its window.
struct PanelWindow {
    window: Arc<Window>,
    surface: Surface,
    panel: Panel,
    frame: Bitmap,
    cursor: (i32, i32),
    last_click: Option<(Instant, (i32, i32))>,
}

enum Drag {
    Move { group: Vec<(WindowKey, (i32, i32))>, start: (i32, i32) },
    Resize { start_size: (i32, i32), start: (i32, i32) },
}

pub struct App {
    pub model: Model,
    fonts: Fonts,
    /// The skinned windows, by `idx`.
    skins: [SkinWin; 3],
    wins: [Option<SkinWindow>; 3],
    popups: Vec<Popup>,
    panels: Vec<PanelWindow>,
    tip: Option<Tooltip>,
    /// The skin's cursors, made into window-system cursors.
    cursors: HashMap<CursorKind, CustomCursor>,
    drag: Option<Drag>,
    capture: Option<WindowKey>,
    mods: ModifiersState,
    last_click: Option<(WindowKey, Instant, (i32, i32))>,
    drops: Vec<(WindowKey, String)>,
    drop_time: Instant,
    seen_pl_dirty: u64,
    next_frame: Instant,
    last_slow: Instant,
    start_minimized: bool,
    popup_focus_check: Option<Instant>,
    /// `POPAMP_SNAPSHOT=<dir>`: write each window as PNG after a few frames and exit.
    snapshot: Option<(std::path::PathBuf, u32)>,
    platform: Platform,
    mpris: Option<crate::mpris::Mpris>,
    /// The scale the windows are shown at now.
    shown_scale: Scale,
}

impl App {
    pub fn new(model: Model, start_minimized: bool, mpris: Option<crate::mpris::Mpris>) -> App {
        let fonts = Fonts::for_skin(&model.skin);
        let shown_scale = Scale::of(model.settings());
        App {
            model,
            fonts,
            skins: [SkinWin::new(WindowKey::Main), SkinWin::new(WindowKey::Eq), SkinWin::new(WindowKey::Playlist)],
            wins: [None, None, None],
            popups: Vec::new(),
            panels: Vec::new(),
            tip: None,
            cursors: HashMap::new(),
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
            snapshot: std::env::var_os("POPAMP_SNAPSHOT").map(|d| (std::path::PathBuf::from(d), 0)),
            platform: Platform::connect(),
            mpris,
            shown_scale,
        }
    }

    fn scale(&self) -> Scale {
        Scale::of(self.model.settings())
    }

    fn logical_size(&self, k: WindowKey) -> (i32, i32) {
        crate::bind::window_size(&self.model, k)
    }

    fn placement(&self, k: WindowKey) -> Placement {
        self.model.skin.layout.window(k).map_or(Placement::Docked, |d| d.placement)
    }

    fn wanted_visible(&self, k: WindowKey) -> bool {
        crate::bind::shown(&self.model, k)
    }

    fn saved_pos(&self, k: WindowKey) -> (i32, i32) {
        crate::bind::position(&self.model, k)
    }

    fn store_pos(&mut self, k: WindowKey, p: (i32, i32)) {
        crate::bind::set_position(&mut self.model, k, p)
    }

    fn win(&self, k: WindowKey) -> Option<&SkinWindow> {
        self.wins[k.index()].as_ref()
    }

    fn win_mut(&mut self, k: WindowKey) -> Option<&mut SkinWindow> {
        self.wins[k.index()].as_mut()
    }

    fn kind_of(&self, id: WindowId) -> Option<WindowKey> {
        WindowKey::ALL.into_iter().find(|&k| self.win(k).is_some_and(|w| w.window.id() == id))
    }

    fn create_windows(&mut self, el: &ActiveEventLoop) {
        let scale = self.scale();
        // Keep windows on a connected monitor.
        let monitors: Vec<(i32, i32, i32, i32)> =
            el.available_monitors().map(|m| (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32)).collect();
        for k in WindowKey::ALL {
            let (w, h) = self.logical_size(k);
            let mut pos = self.saved_pos(k);
            if self.model.settings().keeponscreen && !monitors.is_empty() {
                let on_screen = monitors
                    .iter()
                    .any(|&(mx, my, mw, mh)| pos.0 + 20 > mx && pos.0 < mx + mw - 20 && pos.1 >= my - 5 && pos.1 < my + mh - 14);
                if !on_screen {
                    let (mx, my, _, _) = monitors[0];
                    pos = (mx + 100, my + 100 + scale.px(k.index() as i32 * 116));
                }
            }
            let title = crate::bind::title(k);
            let role = match self.placement(k) {
                Placement::Free => WindowRole::Main,
                _ => WindowRole::Docked,
            };
            let mut attrs = platform::attributes(role)
                .with_title(title)
                .with_inner_size(PhysicalSize::new(scale.px(w) as u32, scale.px(h) as u32))
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
            window.set_cursor(CursorIcon::Default);
            self.wins[k.index()] = Some(SkinWindow {
                window,
                surface,
                pos,
                size: (w, h),
                visible: false,
                focused: false,
                cursor: (0, 0),
                frame: Bitmap::new(w, h, 0),
                shape_sig: None,
                cursor_kind: None,
            });
            self.store_pos(k, pos);
        }
        self.build_cursors(el);
        self.apply_layout(WindowKey::Main);
        if self.start_minimized {
            for k in WindowKey::ALL {
                if let Some(w) = self.win(k) {
                    w.window.set_minimized(true);
                }
            }
        }
    }

    /// Applies sizes, scale and visibility; keeps docked windows attached
    /// when windows change size; `anchor` stays where it is.
    fn apply_layout(&mut self, anchor: WindowKey) {
        let (scale, shown) = (self.scale(), self.shown_scale);
        let old: Vec<Option<Rect>> = WindowKey::ALL
            .iter()
            .map(|&k| {
                let w = self.win(k).filter(|w| w.visible)?;
                Some((w.pos.0, w.pos.1, shown.px(w.size.0), shown.px(w.size.1)))
            })
            .collect();
        let sizes: Vec<(i32, i32)> = WindowKey::ALL
            .iter()
            .map(|&k| {
                let (lw, lh) = self.logical_size(k);
                (scale.px(lw), scale.px(lh))
            })
            .collect();
        let moved = follow_docked(&old, &sizes, anchor.index(), |d| scale.px(shown.skin(d)));
        let new_pos: Vec<(i32, i32)> =
            WindowKey::ALL.iter().zip(moved).map(|(&k, p)| self.win(k).map_or((0, 0), |w| if w.visible { p } else { w.pos })).collect();
        for (i, &k) in WindowKey::ALL.iter().enumerate() {
            let visible = self.wanted_visible(k);
            let (lw, lh) = self.logical_size(k);
            let pos = new_pos[i];
            if let Some(w) = self.win_mut(k) {
                let _ = w.window.request_inner_size(PhysicalSize::new(scale.px(lw) as u32, scale.px(lh) as u32));
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
        self.shown_scale = scale;
        self.update_shapes();
    }

    /// Applies the window shapes of the modes on screen.
    fn update_shapes(&mut self) {
        let scale = self.scale();
        for k in WindowKey::ALL {
            let def = self.model.skin.layout.window(k);
            let shape = def.and_then(|d| d.mode(crate::bind::shaded(&self.model, k)).shape.as_ref());
            let Some(w) = self.wins[k.index()].as_mut() else { continue };
            let size = (scale.px(w.size.0), scale.px(w.size.1));
            let sig = (shape.map_or(0, |s| s as *const Shape as usize), size.0, size.1);
            if w.shape_sig == Some(sig) {
                continue;
            }
            w.shape_sig = Some(sig);
            let mask = shape.and_then(|s| shape_mask(&self.model.skin, s, w.size, scale));
            if let Err(e) = self.platform.set_shape(&w.window, mask.as_ref(), size) {
                log::debug!("shape: {e}");
            }
        }
    }

    fn render(&mut self, k: WindowKey) {
        let scale = self.scale();
        let i = k.index();
        let Some(w) = self.wins[i].as_mut() else { return };
        if !w.visible {
            return;
        }
        let (lw, lh) = w.size;
        // Vector skins are drawn at screen size, classic ones at x1 and scaled.
        let drawn = if self.model.skin.vector { scale } else { Scale::ONE };
        let (fw, fh) = (drawn.px(lw), drawn.px(lh));
        if w.frame.w != fw || w.frame.h != fh {
            w.frame = Bitmap::new(fw, fh, 0);
        }
        let active = w.focused;
        self.skins[k.index()].draw(&self.model, &mut self.fonts, &mut Canvas::new(&mut w.frame, drawn), active);
        let (pw, ph) = (scale.px(lw) as u32, scale.px(lh) as u32);
        let (Some(nw), Some(nh)) = (NonZeroU32::new(pw), NonZeroU32::new(ph)) else { return };
        if w.surface.resize(nw, nh).is_err() {
            return;
        }
        if let Ok(mut buf) = w.surface.buffer_mut() {
            scale.present(&w.frame, &mut buf, pw as usize, ph as usize);
            if let Err(e) = buf.present() {
                log::debug!("present failed: {e}");
            }
        }
    }

    fn redraw_all(&mut self) {
        for k in WindowKey::ALL {
            self.render(k);
        }
        for i in 0..self.popups.len() {
            self.render_popup(i);
        }
    }

    // --- pop-up menus ---------------------------------------------------------

    fn cursor_screen(&self, k: WindowKey) -> (i32, i32) {
        self.win(k).map(|w| (w.pos.0 + w.cursor.0, w.pos.1 + w.cursor.1)).unwrap_or((100, 100))
    }

    fn open_popup(&mut self, el: &ActiveEventLoop, items: Vec<MenuItem>, at: (i32, i32)) {
        if items.is_empty() {
            return;
        }
        let layout = menu::layout(&items, &mut self.fonts.list);
        let (mut x, mut y) = at;
        // Keep the menu on the monitor.
        if let Some(m) = self.win(WindowKey::Main).and_then(|w| w.window.current_monitor()) {
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
        menu::draw(&mut p.frame, &p.items, &p.layout, p.hover, &mut self.fonts.list);
        present_unscaled(&mut p.surface, &p.frame);
    }

    // --- own dialogs ------------------------------------------------------------

    /// Opens the dialog `kind`, or raises it
    /// when it is already open.
    fn open_panel(&mut self, el: &ActiveEventLoop, kind: PanelKind) {
        if let Some(p) = self.panels.iter().find(|p| p.panel.kind() == kind) {
            p.window.focus_window();
            return;
        }
        let panel = Panel::open(kind, &self.model);
        let (w, h) = (panel.form().w, panel.form().h);
        // Centred on the main window's monitor, so the title bar is on screen.
        let monitor = self.win(WindowKey::Main).and_then(|m| m.window.current_monitor());
        let (cx, cy) = monitor
            .map(|m| (m.position().x + m.size().width as i32 / 2, m.position().y + m.size().height as i32 / 2))
            .unwrap_or((640, 400));
        let attrs = platform::attributes(WindowRole::Dialog)
            .with_title(panel.title())
            .with_inner_size(PhysicalSize::new(w as u32, h as u32))
            .with_position(PhysicalPosition::new((cx - w / 2).max(0), (cy - h / 2).max(0)));
        let Ok(window) = el.create_window(attrs).map(Arc::new) else { return };
        let Ok(ctx) = softbuffer::Context::new(window.clone()) else { return };
        let Ok(surface) = Surface::new(&ctx, window.clone()) else { return };
        let frame = Bitmap::new(w, h, 0);
        self.panels.push(PanelWindow { window, surface, panel, frame, cursor: (0, 0), last_click: None });
        self.panel_changed(self.panels.len() - 1);
    }

    /// Starts the network work panel `i` asked for and redraws it.
    fn panel_changed(&mut self, i: usize) {
        for job in self.panels[i].panel.take_jobs() {
            crate::museum::start(job, self.model.settings().proxy.clone(), self.model.events().clone());
        }
        self.render_panel(i);
    }

    fn render_panel(&mut self, i: usize) {
        let Some(p) = self.panels.get_mut(i) else { return };
        p.panel.form().draw(&mut p.frame, &mut self.fonts.list);
        present_unscaled(&mut p.surface, &p.frame);
    }

    fn panel_event(&mut self, el: &ActiveEventLoop, i: usize, ev: WindowEvent) {
        let p = &mut self.panels[i];
        let (x, y) = p.cursor;
        let form = p.panel.form_mut();
        let out = match ev {
            WindowEvent::CloseRequested => Event::Cancel,
            WindowEvent::RedrawRequested => Event::None,
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                p.cursor = (position.x as i32, position.y as i32);
                return;
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let now = Instant::now();
                let double = p.last_click.is_some_and(|(t, at)| now - t < Duration::from_millis(400) && at == (x, y));
                p.last_click = Some((now, (x, y)));
                form.mouse_down(x, y, double)
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => form.mouse_up(x, y),
            WindowEvent::MouseWheel { delta, .. } => {
                form.wheel(match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 20.0).round() as i32,
                });
                Event::None
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match form_key(&event, self.mods.control_key() || self.mods.alt_key()) {
                    Some(k) => form.key(k),
                    None => return,
                }
            }
            _ => return,
        };
        if !p.panel.handle(out, &mut self.model) {
            self.panels.remove(i);
        } else {
            self.panel_changed(i);
        }
        self.after_model(el);
        self.redraw_all();
    }

    /// Turns the skin's cursors into window-system cursors (none when the
    /// skin has none or `usecursors` is off).
    fn build_cursors(&mut self, el: &ActiveEventLoop) {
        self.cursors.clear();
        if self.model.settings().usecursors {
            for (kind, c) in &self.model.skin.cursors {
                match CustomCursor::from_rgba(c.rgba.clone(), c.w, c.h, c.hot_x, c.hot_y) {
                    Ok(src) => {
                        self.cursors.insert(*kind, el.create_custom_cursor(src));
                    }
                    Err(e) => log::warn!("cursor {}: {e}", kind.file_name()),
                }
            }
        }
        for w in self.wins.iter_mut().flatten() {
            w.cursor_kind = None;
        }
    }

    /// Shows the skin cursor for the control under the mouse in window `k`.
    fn update_cursor(&mut self, k: WindowKey) {
        let input = self.input(k, false, false);
        let kind = self.skins[k.index()].cursor(&self.model, input.x, input.y);
        let custom = self.cursors.get(&kind).or_else(|| self.cursors.get(&CursorKind::Normal)).cloned();
        let Some(w) = self.wins[k.index()].as_mut() else { return };
        if w.cursor_kind == Some(kind) {
            return;
        }
        w.cursor_kind = Some(kind);
        match custom {
            Some(c) => w.window.set_cursor(c),
            None => w.window.set_cursor(CursorIcon::Default),
        }
    }

    /// Starts the tooltip timer when the mouse rests on a new control.
    fn hover_tooltip(&mut self, k: WindowKey, x: i32, y: i32) {
        if k != WindowKey::Main || self.capture.is_some() {
            self.tip = None;
            return;
        }
        let text = self.skins[k.index()].tooltip(&self.model, x, y).unwrap_or_else(|| format!("Popamp {}", env!("CARGO_PKG_VERSION")));
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
        let frame = menu::tooltip(&text, &mut self.fonts.list);
        let (x, y) = self.cursor_screen(WindowKey::Main);
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
        let frame = menu::tooltip(text, &mut self.fonts.list);
        present_unscaled(surface, &frame);
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

    fn rect(&self, k: WindowKey) -> Option<(i32, i32, i32, i32)> {
        let w = self.win(k)?;
        if !w.visible {
            return None;
        }
        let s = self.scale();
        Some((w.pos.0, w.pos.1, s.px(w.size.0), s.px(w.size.1)))
    }

    fn docked_group(&self, root: WindowKey) -> Vec<WindowKey> {
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
            for k in WindowKey::ALL {
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

    fn begin_move(&mut self, k: WindowKey) {
        // A free window takes the windows docked to it along.
        let group = if self.placement(k) == Placement::Free { self.docked_group(k) } else { vec![k] };
        let start = self.cursor_screen(k);
        let group = group.into_iter().filter_map(|g| self.win(g).map(|w| (g, w.pos))).collect();
        self.drag = Some(Drag::Move { group, start });
    }

    fn snap_delta(&self, group: &[(WindowKey, (i32, i32))], dx: i32, dy: i32) -> (i32, i32) {
        let s = self.model.settings();
        if !s.snap {
            return (dx, dy);
        }
        let snap = s.snaplen;
        let scale = self.scale();
        let in_group = |k: WindowKey| group.iter().any(|(g, _)| *g == k);
        let mut targets: Vec<(i32, i32, i32, i32)> =
            WindowKey::ALL.iter().filter(|&&k| !in_group(k)).filter_map(|&k| self.rect(k)).collect();
        if let Some(m) = self.win(WindowKey::Main).and_then(|w| w.window.current_monitor()) {
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
            let (gw, gh) = (scale.px(w.size.0), scale.px(w.size.1));
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

    fn drag_to(&mut self, k: WindowKey) {
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
                let (dx, dy) = (s.skin(cursor.0 - start.0), s.skin(cursor.1 - start.1));
                if crate::bind::resize(&mut self.model, WindowKey::Playlist, *start_size, dx, dy) {
                    self.apply_layout(WindowKey::Playlist);
                }
            }
            None => {}
        }
    }

    // --- input -----------------------------------------------------------------

    fn input(&self, k: WindowKey, right: bool, double: bool) -> Input {
        let s = self.scale();
        let c = self.win(k).map(|w| w.cursor).unwrap_or((0, 0));
        Input { x: s.skin(c.0), y: s.skin(c.1), right, shift: self.mods.shift_key(), ctrl: self.mods.control_key(), double }
    }

    fn handle_out(&mut self, el: &ActiveEventLoop, k: WindowKey, out: Out) {
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

    fn mouse_down(&mut self, el: &ActiveEventLoop, k: WindowKey, button: MouseButton) {
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
        let out = self.skins[k.index()].mouse_down(&mut self.model, &input);
        self.handle_out(el, k, out);
    }

    fn mouse_up(&mut self, el: &ActiveEventLoop, k: WindowKey) {
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
        let out = self.skins[target.index()].mouse_up(&mut self.model, &input);
        self.handle_out(el, target, out);
    }

    fn mouse_moved(&mut self, k: WindowKey) {
        if self.drag.is_some() {
            self.drag_to(k);
            return;
        }
        self.update_cursor(k);
        let target = self.capture.unwrap_or(k);
        let input = self.input(target, false, false);
        self.hover_tooltip(target, input.x, input.y);
        let changed = self.skins[target.index()].mouse_move(&mut self.model, input.x, input.y);
        if changed {
            self.render(target);
            if target == WindowKey::Eq {
                self.render(WindowKey::Main);
            }
        }
    }

    fn key(&mut self, el: &ActiveEventLoop, k: WindowKey, ev: &KeyEvent) {
        if ev.state != ElementState::Pressed {
            return;
        }
        let PhysicalKey::Code(code) = ev.physical_key else { return };
        if !self.popups.is_empty() {
            self.popup_key(el, code);
            return;
        }
        if self.skins[k.index()].popup_open() && code == KeyCode::Escape {
            self.skins[k.index()].close_popup();
            self.render(k);
            return;
        }
        let (ctrl, shift, alt) = (self.mods.control_key(), self.mods.shift_key(), self.mods.alt_key());
        use Command as C;
        use KeyCode as K;
        // Window-specific keys first.
        if k == WindowKey::Eq && !ctrl && !alt {
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
        if k == WindowKey::Eq && ctrl && alt && code == K::KeyS {
            return self.run(el, C::EqPresetLoad);
        }
        if k == WindowKey::Playlist {
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
                self.skins[WindowKey::Playlist.index()].key_nav(&mut self.model, n, shift);
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
        if k == WindowKey::Eq && ctrl && code == K::KeyW {
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
            K::KeyI if ctrl && alt => Some(C::RemoveBookmark),
            K::KeyI if alt && k == WindowKey::Playlist => Some(C::BookmarkSelected),
            K::KeyI if alt => Some(C::AddBookmark),
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

    fn redraw_after(&mut self, el: &ActiveEventLoop, k: WindowKey) {
        self.after_model(el);
        self.render(k);
        self.render(WindowKey::Main);
    }

    fn cycle_focus(&mut self, from: WindowKey) {
        let start = from.index();
        for i in 1..=3 {
            let k = WindowKey::ALL[(start + i) % 3];
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
                    let _ = std::thread::Builder::new().name("popamp-dialog".into()).spawn(move || {
                        let values = dialogs::execute(&req);
                        events.send(AppEvent::Dialog(DialogResult { kind, values }));
                    });
                }
                UiReq::Panel(kind) => self.open_panel(el, kind),
                UiReq::Menu(items) => {
                    let at = self.cursor_screen(WindowKey::Main);
                    self.open_popup(el, items, at);
                }
                UiReq::Layout => {
                    self.apply_layout(WindowKey::Main);
                    self.redraw_all();
                }
                UiReq::Minimize => {
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win(k).filter(|w| w.visible) {
                            w.window.set_minimized(true);
                        }
                    }
                }
                UiReq::Raise => {
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win(k).filter(|w| w.visible) {
                            w.window.set_minimized(false);
                        }
                    }
                    if let Some(w) = self.win(WindowKey::Main) {
                        w.window.focus_window();
                    }
                }
                UiReq::Quit => {
                    self.model.save_session();
                    el.exit();
                }
                UiReq::SkinChanged => {
                    self.fonts = Fonts::for_skin(&self.model.skin);
                    self.build_cursors(el);
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win_mut(k) {
                            w.shape_sig = None;
                        }
                    }
                    // A vector skin may size its windows differently.
                    self.apply_layout(WindowKey::Main);
                    self.redraw_all();
                }
                UiReq::AlwaysOnTop => {
                    let level = if self.model.settings().aot { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal };
                    for k in WindowKey::ALL {
                        if let Some(w) = self.win(k) {
                            w.window.set_window_level(level);
                        }
                    }
                    self.render(WindowKey::Main);
                }
            }
        }
        if let Some(m) = &mut self.mpris {
            m.update(self.model.mpris_snapshot());
        }
        if self.model.pl_dirty != self.seen_pl_dirty {
            self.seen_pl_dirty = self.model.pl_dirty;
            for i in 0..self.panels.len() {
                self.panels[i].panel.playlist_changed(&self.model);
                self.render_panel(i);
            }
            if let Some(cur) = self.model.playlist.current() {
                if self.model.status.playing_id.is_some() {
                    let _ = cur;
                }
            }
            self.render(WindowKey::Playlist);
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
            WindowKey::Playlist => {
                // Insert at the row under the cursor.
                let input = self.input(WindowKey::Playlist, false, false);
                let at = self.skins[WindowKey::Playlist.index()].row_at(&self.model, input.y);
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
        self.skins[WindowKey::Main.index()].tick(&self.model, &mut self.fonts);
        self.render(WindowKey::Main);
        if self.last_slow.elapsed() >= Duration::from_millis(500) {
            self.last_slow = Instant::now();
            self.model.reload_skin_if_changed();
            if state != PlayState::Stopped || settings.pe_width >= 350 {
                self.render(WindowKey::Playlist);
            }
        } else if settings.pe_width >= 350 && state == PlayState::Playing {
            self.render(WindowKey::Playlist);
        }
    }
}

/// Shows `frame` pixel for pixel: menus, tooltips and dialogs are drawn at
/// the screen's resolution.
fn present_unscaled(surface: &mut Surface, frame: &Bitmap) {
    let (w, h) = (frame.w as u32, frame.h as u32);
    let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) else { return };
    if surface.resize(nw, nh).is_ok() {
        if let Ok(mut buf) = surface.buffer_mut() {
            Scale::ONE.present(frame, &mut buf, w as usize, h as usize);
            let _ = buf.present();
        }
    }
}

/// The key for a dialog form; `None` for keys it ignores and for
/// shortcuts with Ctrl or Alt.
fn form_key(ev: &KeyEvent, with_modifier: bool) -> Option<widgets::Key> {
    use widgets::Key as F;
    Some(match &ev.logical_key {
        Key::Named(n) => match n {
            NamedKey::Enter => F::Enter,
            NamedKey::Escape => F::Escape,
            NamedKey::Tab => F::Tab,
            NamedKey::Backspace => F::Backspace,
            NamedKey::Delete => F::Delete,
            NamedKey::ArrowLeft => F::Left,
            NamedKey::ArrowRight => F::Right,
            NamedKey::ArrowUp => F::Up,
            NamedKey::ArrowDown => F::Down,
            NamedKey::Home => F::Home,
            NamedKey::End => F::End,
            NamedKey::PageUp => F::PageUp,
            NamedKey::PageDown => F::PageDown,
            NamedKey::Space if !with_modifier => F::Char(' '),
            _ => return None,
        },
        _ if with_modifier => return None,
        _ => F::Char(ev.text.as_ref()?.chars().next()?),
    })
}

/// The application logo as the window icon (taskbar, alt-tab).
fn window_icon() -> Option<winit::window::Icon> {
    static PNG: &[u8] = include_bytes!("../../../assets/logo/png/popamp-64.png");
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
                    for k in WindowKey::ALL {
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
                    let any_focused = WindowKey::ALL.iter().any(|&k| self.win(k).is_some_and(|w| w.focused));
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
                    self.skins[WindowKey::Playlist.index()].ensure_visible(&self.model, cur);
                }
                self.render(WindowKey::Main);
                self.render(WindowKey::Playlist);
            }
            AppEvent::Meta(id, m) => self.model.on_meta(id, m),
            AppEvent::Expanded { items, mode } => self.model.on_expanded(items, mode),
            AppEvent::Dialog(r) => self.model.on_dialog(r),
            AppEvent::Museum(f) => {
                if let Some(i) = self.panels.iter().position(|p| p.panel.kind() == PanelKind::SkinMuseum) {
                    self.panels[i].panel.fetched(f, &mut self.model);
                    self.panel_changed(i);
                }
            }
            AppEvent::Remote(req) => {
                let reply = self.model.on_remote(req.remotes);
                let _ = req.reply.send(reply);
            }
        }
        self.after_model(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        if let Some(i) = self.panels.iter().position(|p| p.window.id() == id) {
            return self.panel_event(el, i, ev);
        }
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
                if !f && self.skins[k.index()].popup_open() {
                    self.skins[k.index()].close_popup();
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
                    self.skins[k.index()].wheel(&mut self.model, input.x, input.y, steps);
                    self.render(k);
                    self.render(WindowKey::Main);
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

/// The pixels of a window `size` skin pixels big that `shape` shows.
fn shape_mask(skin: &Skin, shape: &Shape, size: (i32, i32), scale: Scale) -> Option<Mask> {
    let (w, h) = size;
    match shape {
        Shape::Polygons(polys) => Some(Mask { w, h, on: region_mask(polys, w, h), scale }),
        Shape::Picture(Paint { picture: Picture::Svg(id), src, .. }) => {
            let (pw, ph) = (scale.px(w), scale.px(h));
            let r = skin.svgs.raster(*id, *src, pw, ph)?;
            Some(Mask { w: pw, h: ph, on: r.px.iter().map(|p| p >> 24 >= 128).collect(), scale: Scale::ONE })
        }
        Shape::Picture(_) => None,
    }
}

/// Screen rectangle of a shown window: x, y, width, height.
type Rect = (i32, i32, i32, i32);

/// New positions after windows change size. `anchor`, then the first shown
/// window of every other docked group, stays put; a window docked to a placed
/// one goes back against the same edge, its offset along that edge passed
/// through `rescale`.
fn follow_docked(old: &[Option<Rect>], sizes: &[(i32, i32)], anchor: usize, rescale: impl Fn(i32) -> i32) -> Vec<(i32, i32)> {
    let mut pos: Vec<Option<(i32, i32)>> = vec![None; old.len()];
    for root in std::iter::once(anchor).chain(0..old.len()) {
        let Some(r) = old[root].filter(|_| pos[root].is_none()) else { continue };
        pos[root] = Some((r.0, r.1));
        let mut todo = vec![root];
        while let Some(a) = todo.pop() {
            let (Some(ra), Some(pa)) = (old[a], pos[a]) else { continue };
            let now = (pa.0, pa.1, sizes[a].0, sizes[a].1);
            for b in 0..old.len() {
                let Some(rb) = old[b].filter(|_| pos[b].is_none()) else { continue };
                pos[b] = reattach(ra, rb, now, sizes[b], &rescale);
                if pos[b].is_some() {
                    todo.push(b);
                }
            }
        }
    }
    pos.into_iter().map(|p| p.unwrap_or((0, 0))).collect()
}

/// Where `b`, which touched `a` before, goes now that `a` is at `now` and
/// `b` has `size`; `None` if they did not touch.
fn reattach(a: Rect, b: Rect, now: Rect, size: (i32, i32), rescale: &impl Fn(i32) -> i32) -> Option<(i32, i32)> {
    let columns_overlap = b.0 < a.0 + a.2 && b.0 + b.2 > a.0;
    let rows_overlap = b.1 < a.1 + a.3 && b.1 + b.3 > a.1;
    let touch = |gap: i32| gap.abs() <= 1;
    let x = now.0 + rescale(b.0 - a.0);
    let y = now.1 + rescale(b.1 - a.1);
    let sides = [
        (columns_overlap && touch(b.1 - (a.1 + a.3)), (x, now.1 + now.3)),  // below
        (columns_overlap && touch(a.1 - (b.1 + b.3)), (x, now.1 - size.1)), // above
        (rows_overlap && touch(b.0 - (a.0 + a.2)), (now.0 + now.2, y)),     // right
        (rows_overlap && touch(a.0 - (b.0 + b.2)), (now.0 - size.0, y)),    // left
    ];
    sides.into_iter().find(|s| s.0).map(|s| s.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x15(d: i32) -> i32 {
        d * 3 / 2
    }

    #[test]
    fn a_docked_chain_moves_down_as_a_whole() {
        // Main, EQ and playlist stacked at x1, then all grown to x1.5.
        let old = [Some((100, 100, 275, 116)), Some((100, 216, 275, 116)), Some((100, 332, 275, 232))];
        let sizes = [(413, 174), (413, 174), (413, 348)];
        assert_eq!(follow_docked(&old, &sizes, 0, x15), [(100, 100), (100, 274), (100, 448)]);
    }

    #[test]
    fn a_window_on_the_left_moves_left_as_it_grows() {
        // Playlist left of main, EQ under main; x1 to x1.5 and back.
        let old = [Some((500, 0, 275, 116)), Some((500, 116, 275, 116)), Some((225, 0, 275, 232))];
        let sizes = [(413, 174), (413, 174), (413, 348)];
        let grown = follow_docked(&old, &sizes, 0, x15);
        assert_eq!(grown, [(500, 0), (500, 174), (87, 0)]);
        let old = [Some((500, 0, 413, 174)), Some((500, 174, 413, 174)), Some((87, 0, 413, 348))];
        let sizes = [(275, 116), (275, 116), (275, 232)];
        assert_eq!(follow_docked(&old, &sizes, 0, |d| d * 2 / 3), [(500, 0), (500, 116), (225, 0)]);
    }

    #[test]
    fn offsets_along_an_edge_scale_with_the_windows() {
        // EQ right of main and 20 pixels lower.
        let old = [Some((0, 0, 275, 116)), Some((275, 20, 275, 116)), None];
        let sizes = [(413, 174), (413, 174), (0, 0)];
        assert_eq!(follow_docked(&old, &sizes, 0, x15), [(0, 0), (413, 30), (0, 0)]);
    }

    #[test]
    fn shading_main_pulls_the_window_below_up_and_leaves_the_side_one() {
        let old = [Some((500, 0, 275, 116)), Some((500, 116, 275, 116)), Some((225, 0, 275, 232))];
        let sizes = [(275, 14), (275, 116), (275, 232)];
        assert_eq!(follow_docked(&old, &sizes, 0, |d| d), [(500, 0), (500, 14), (225, 0)]);
    }

    #[test]
    fn the_resized_window_stays_and_pushes_its_neighbour() {
        // Playlist left of main grows 25 wider: main moves right, not the playlist left.
        let old = [Some((500, 0, 275, 116)), None, Some((225, 0, 275, 232))];
        let sizes = [(275, 116), (0, 0), (300, 232)];
        assert_eq!(follow_docked(&old, &sizes, 2, |d| d), [(525, 0), (0, 0), (225, 0)]);
    }

    #[test]
    fn detached_windows_stay_put() {
        let old = [Some((0, 0, 275, 116)), None, Some((500, 500, 275, 232))];
        let sizes = [(413, 174), (0, 0), (413, 348)];
        assert_eq!(follow_docked(&old, &sizes, 0, x15), [(0, 0), (0, 0), (500, 500)]);
    }
}
