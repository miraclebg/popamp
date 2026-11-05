//! Popamp's own dialog windows: Jump to file, File info, Preferences,
//! Plug-ins and the Skin Museum.
//! Each is a [`Form`] plus the logic that turns its events into changes to
//! the [`Model`]; the windows themselves live in `app.rs`.

use crate::model::{Model, UiReq};
use crate::museum::{self, Fetched, Job};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON, ROW_H};
use popamp_core::config::Settings;
use popamp_core::Command;
use popamp_plugins::installed::{Installed, State};
use popamp_skin::Bitmap;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    JumpToFile,
    FileInfo,
    Preferences,
    Plugins,
    SkinMuseum,
}

/// What a button or check box in a panel stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Ok,
    Close,
    Flag(Flag),
    Search,
    More,
    /// The check box of the n-th plug-in.
    Plugin(usize),
}

pub enum Panel {
    Jump(JumpToFile),
    Info(FileInfo),
    Prefs(Preferences),
    Plugins(PluginsPanel),
    Museum(SkinMuseum),
}

impl Panel {
    pub fn open(kind: PanelKind, model: &Model) -> Panel {
        match kind {
            PanelKind::JumpToFile => Panel::Jump(JumpToFile::new(model)),
            PanelKind::FileInfo => Panel::Info(FileInfo::new(model)),
            PanelKind::Preferences => Panel::Prefs(Preferences::new(model.settings())),
            PanelKind::Plugins => Panel::Plugins(PluginsPanel::new(model)),
            PanelKind::SkinMuseum => Panel::Museum(SkinMuseum::new()),
        }
    }

    pub fn kind(&self) -> PanelKind {
        match self {
            Panel::Jump(_) => PanelKind::JumpToFile,
            Panel::Info(_) => PanelKind::FileInfo,
            Panel::Prefs(_) => PanelKind::Preferences,
            Panel::Plugins(_) => PanelKind::Plugins,
            Panel::Museum(_) => PanelKind::SkinMuseum,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Panel::Jump(_) => "Jump to file",
            Panel::Info(_) => "File info",
            Panel::Prefs(_) => "Popamp Preferences",
            Panel::Plugins(_) => "Plug-ins",
            Panel::Museum(_) => "Winamp Skin Museum",
        }
    }

    pub fn form(&self) -> &Form<Act> {
        match self {
            Panel::Jump(p) => &p.form,
            Panel::Info(p) => &p.form,
            Panel::Prefs(p) => &p.form,
            Panel::Plugins(p) => &p.form,
            Panel::Museum(p) => &p.form,
        }
    }

    pub fn form_mut(&mut self) -> &mut Form<Act> {
        match self {
            Panel::Jump(p) => &mut p.form,
            Panel::Info(p) => &mut p.form,
            Panel::Prefs(p) => &mut p.form,
            Panel::Plugins(p) => &mut p.form,
            Panel::Museum(p) => &mut p.form,
        }
    }

    /// Acts on what happened in the form. Returns `false` when the panel
    /// should close.
    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match self {
            Panel::Jump(p) => p.handle(ev, model),
            Panel::Info(_) => !matches!(ev, Event::Clicked(_) | Event::Cancel),
            Panel::Prefs(p) => p.handle(ev, model),
            Panel::Plugins(p) => p.handle(ev, model),
            Panel::Museum(p) => p.handle(ev, model),
        }
    }

    /// Network work the panel asked for since the last call.
    pub fn take_jobs(&mut self) -> Vec<Job> {
        match self {
            Panel::Museum(p) => std::mem::take(&mut p.jobs),
            _ => Vec::new(),
        }
    }

    /// A background job finished.
    pub fn fetched(&mut self, f: Fetched, model: &mut Model) {
        if let Panel::Museum(p) = self {
            p.fetched(f, model);
        }
    }

    /// The playlist changed while the panel is open.
    pub fn playlist_changed(&mut self, model: &Model) {
        if let Panel::Jump(p) = self {
            p.refilter(model);
        }
    }
}

fn button(form: &mut Form<Act>, x: i32, y: i32, label: &str, action: Act, default: bool) {
    form.add(Rect::new(x, y, BUTTON.0, BUTTON.1), Widget::Button { label: label.into(), action, default });
}

// --- Jump to file ---------------------------------------------------------------

/// Playlist positions whose title contains every word of `query`, ignoring case.
pub fn search<'a>(titles: impl Iterator<Item = &'a str>, query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    titles
        .enumerate()
        .filter(|(_, t)| {
            let t = t.to_lowercase();
            words.iter().all(|w| t.contains(w))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Search the playlist as you type; Enter plays the selected entry.
pub struct JumpToFile {
    form: Form<Act>,
    search: usize,
    list: usize,
    /// The playlist position of each visible row.
    shown: Vec<usize>,
}

impl JumpToFile {
    fn new(model: &Model) -> JumpToFile {
        let (w, list_h) = (460, 4 + 16 * ROW_H);
        let mut form = Form::new(w, 10 + 24 + 8 + list_h + 10 + BUTTON.1 + 10);
        form.add(Rect::new(10, 10, 70, 24), Widget::Label { text: "Search for:".into(), dim: false });
        let search = form.add(Rect::new(80, 10, w - 90, 24), Widget::Input { text: String::new(), cursor: 0 });
        let list = form.add(Rect::new(10, 42, w - 20, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        let by = 42 + list_h + 10;
        button(&mut form, w - 2 * BUTTON.0 - 16, by, "Jump", Act::Ok, true);
        button(&mut form, w - BUTTON.0 - 10, by, "Close", Act::Close, false);
        let mut p = JumpToFile { form, search, list, shown: vec![] };
        p.refilter(model);
        p
    }

    /// Lists the entries matching the search, keeping the selected entry (or
    /// else the playing one) selected.
    fn refilter(&mut self, model: &Model) {
        let keep = self.form.selected(self.list).and_then(|r| self.shown.get(r).copied()).or(model.playlist.current());
        let entries = model.playlist.entries();
        self.shown = search(entries.iter().map(|e| e.display.as_str()), self.form.text(self.search));
        let rows = self.shown.iter().map(|&i| format!("{}. {}", i + 1, entries[i].display)).collect();
        let select = keep.and_then(|k| self.shown.iter().position(|&i| i == k)).unwrap_or(0);
        self.form.set_rows(self.list, rows, Some(select));
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Edited => {
                // A new search starts from the best match, not the old selection.
                self.form.set_rows(self.list, vec![], None);
                self.refilter(model);
                true
            }
            Event::Clicked(Act::Ok) | Event::Activated => match self.form.selected(self.list).and_then(|r| self.shown.get(r)) {
                Some(&i) => {
                    model.play_index(i);
                    false
                }
                None => true,
            },
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }
}

// --- File info ------------------------------------------------------------------

/// Tags and format of the selected (or playing) entry.
pub struct FileInfo {
    form: Form<Act>,
}

impl FileInfo {
    fn new(model: &Model) -> FileInfo {
        let fields = model.file_info_fields();
        let (w, key_w) = (520, 110);
        let mut form = Form::new(w, 10 + fields.len() as i32 * ROW_H + 12 + BUTTON.1 + 10);
        for (n, (k, v)) in fields.into_iter().enumerate() {
            let y = 10 + n as i32 * ROW_H;
            form.add(Rect::new(10, y, key_w - 10, ROW_H), Widget::Label { text: k, dim: true });
            form.add(Rect::new(key_w, y, w - key_w - 10, ROW_H), Widget::Label { text: v, dim: false });
        }
        let by = form.h - BUTTON.1 - 10;
        button(&mut form, w - BUTTON.0 - 10, by, "Close", Act::Close, true);
        FileInfo { form }
    }
}

// --- Preferences ----------------------------------------------------------------

/// The on/off preferences. Check boxes take effect at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Snap,
    EasyMove,
    KeepOnScreen,
    AlwaysOnTop,
    DoubleSize,
    SkinCursors,
    ScrollTitle,
    DimInactive,
    ShowNumbers,
    ZeroPad,
    AutoAdvance,
}

impl Flag {
    const WINDOWS: [Flag; 6] = [Flag::Snap, Flag::EasyMove, Flag::KeepOnScreen, Flag::AlwaysOnTop, Flag::DoubleSize, Flag::SkinCursors];
    const DISPLAY: [Flag; 5] = [Flag::ScrollTitle, Flag::DimInactive, Flag::ShowNumbers, Flag::ZeroPad, Flag::AutoAdvance];

    fn label(self) -> &'static str {
        match self {
            Flag::Snap => "Snap windows together",
            Flag::EasyMove => "Easy move",
            Flag::KeepOnScreen => "Keep windows on screen",
            Flag::AlwaysOnTop => "Always on top",
            Flag::DoubleSize => "Double size",
            Flag::SkinCursors => "Use skin cursors",
            Flag::ScrollTitle => "Scroll song title",
            Flag::DimInactive => "Dim inactive playlist",
            Flag::ShowNumbers => "Show playlist numbers",
            Flag::ZeroPad => "Zero-pad numbers",
            Flag::AutoAdvance => "Advance to next track",
        }
    }

    /// The command that toggles the preference elsewhere (menu, keys), when
    /// changing it needs more than storing it.
    fn command(self) -> Option<Command> {
        match self {
            Flag::EasyMove => Some(Command::ToggleEasyMove),
            Flag::AlwaysOnTop => Some(Command::ToggleAlwaysOnTop),
            Flag::DoubleSize => Some(Command::ToggleDoubleSize),
            Flag::ScrollTitle => Some(Command::ToggleAutoscroll),
            Flag::AutoAdvance => Some(Command::ToggleManualAdvance),
            _ => None,
        }
    }

    fn slot(self, s: &mut Settings) -> &mut bool {
        match self {
            Flag::Snap => &mut s.snap,
            Flag::EasyMove => &mut s.easymove,
            Flag::KeepOnScreen => &mut s.keeponscreen,
            Flag::AlwaysOnTop => &mut s.aot,
            Flag::DoubleSize => &mut s.dsize,
            Flag::SkinCursors => &mut s.usecursors,
            Flag::ScrollTitle => &mut s.autoscrollname,
            Flag::DimInactive => &mut s.dim_inactive,
            Flag::ShowNumbers => &mut s.shownumsinpl,
            Flag::ZeroPad => &mut s.zeropadplnum,
            Flag::AutoAdvance => &mut s.pladv,
        }
    }

    pub fn get(self, s: &Settings) -> bool {
        // A copy keeps the list of fields in one place, `slot`; it is
        // only read when a check box is drawn or clicked.
        *self.slot(&mut s.clone())
    }

    /// Sets the preference the way its menu entry or key would, so that
    /// windows, flashes and the engine follow.
    pub fn set(self, model: &mut Model, on: bool) {
        if self.get(model.settings()) == on {
            return;
        }
        match self.command() {
            Some(c) => model.exec(c),
            None => *self.slot(model.settings_mut()) = on,
        }
        if self == Flag::SkinCursors {
            model.ui.push(UiReq::SkinChanged);
        }
        model.pl_dirty += 1;
    }
}

/// The preferences typed in. They apply on OK; Cancel forgets the edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    FontSize,
    TitleFormat,
    FadeMs,
    BufferMs,
    Proxy,
}

impl Field {
    const ALL: [Field; 5] = [Field::FontSize, Field::TitleFormat, Field::FadeMs, Field::BufferMs, Field::Proxy];

    fn label(self) -> &'static str {
        match self {
            Field::FontSize => "Playlist font size",
            Field::TitleFormat => "Title format",
            Field::FadeMs => "Fade-out (ms)",
            Field::BufferMs => "Output buffer (ms)",
            Field::Proxy => "HTTP proxy",
        }
    }

    fn get(self, s: &Settings) -> String {
        match self {
            Field::FontSize => s.pe_fontsize.to_string(),
            Field::TitleFormat => s.titlefmt.clone(),
            Field::FadeMs => s.fade_ms.to_string(),
            Field::BufferMs => s.buffer_ms.to_string(),
            Field::Proxy => s.proxy.clone(),
        }
    }

    /// Checks `text` without changing anything.
    fn check(self, text: &str) -> Result<(), String> {
        let number = |what| text.trim().parse::<u32>().map(drop).map_err(|_| format!("{what}: enter a whole number"));
        match self {
            Field::FontSize => number("Font size"),
            Field::FadeMs => number("Fade-out"),
            Field::BufferMs => number("Output buffer"),
            Field::TitleFormat if text.trim().is_empty() => Err("Title format: must not be empty".into()),
            Field::TitleFormat | Field::Proxy => Ok(()),
        }
    }

    /// Stores a checked value; numbers are kept in range by the settings.
    fn apply(self, model: &mut Model, text: &str) {
        let n = || text.trim().parse::<u32>().unwrap_or(0);
        match self {
            Field::FontSize => model.settings_mut().pe_fontsize = n().min(1000) as i32,
            Field::FadeMs => model.settings_mut().fade_ms = n(),
            Field::BufferMs => model.settings_mut().buffer_ms = n(),
            Field::Proxy => model.settings_mut().proxy = text.trim().to_string(),
            Field::TitleFormat => {
                if text != model.settings().titlefmt {
                    model.set_title_format(text);
                }
            }
        }
    }
}

pub struct Preferences {
    form: Form<Act>,
    fields: Vec<(Field, usize)>,
    status: usize,
}

impl Preferences {
    fn new(s: &Settings) -> Preferences {
        let w = 480;
        let mut form = Form::new(w, 0);
        let mut y = 10;
        for (title, flags) in [("Windows", &Flag::WINDOWS[..]), ("Display and playback", &Flag::DISPLAY[..])] {
            let rows = flags.len().div_ceil(2) as i32;
            form.add(Rect::new(10, y, w - 20, 22 + rows * 22), Widget::Group { label: title.into() });
            for (n, &f) in flags.iter().enumerate() {
                let (cx, cy) = (22 + (n as i32 % 2) * (w / 2 - 12), y + 20 + (n as i32 / 2) * 22);
                form.add(
                    Rect::new(cx, cy, w / 2 - 30, 20),
                    Widget::Check { label: f.label().into(), checked: f.get(s), action: Act::Flag(f) },
                );
            }
            y += 30 + rows * 22;
        }
        let mut fields = Vec::new();
        for f in Field::ALL {
            form.add(Rect::new(14, y, 136, 24), Widget::Label { text: f.label().into(), dim: false });
            let text = f.get(s);
            let cursor = text.chars().count();
            fields.push((f, form.add(Rect::new(150, y, w - 160, 24), Widget::Input { text, cursor })));
            y += 30;
        }
        y += 6;
        let status = form.add(Rect::new(10, y, w - 2 * BUTTON.0 - 36, BUTTON.1), Widget::Label { text: String::new(), dim: false });
        button(&mut form, w - 2 * BUTTON.0 - 16, y, "OK", Act::Ok, true);
        button(&mut form, w - BUTTON.0 - 10, y, "Cancel", Act::Close, false);
        form.h = y + BUTTON.1 + 10;
        Preferences { form, fields, status }
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Toggled(Act::Flag(f), on) => {
                f.set(model, on);
                true
            }
            Event::Clicked(Act::Ok) => self.apply(model),
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }

    /// Stores the typed values when all are valid; otherwise shows the first
    /// problem and keeps the window open.
    fn apply(&mut self, model: &mut Model) -> bool {
        let checked = self.fields.iter().try_for_each(|&(f, i)| f.check(self.form.text(i)));
        if let Err(e) = checked {
            self.form.set_label(self.status, e);
            return true;
        }
        let proxy = model.settings().proxy.clone();
        for &(f, i) in &self.fields {
            f.apply(model, self.form.text(i));
        }
        model.settings_mut().sanitize();
        model.push_engine_settings();
        if model.settings().proxy != proxy {
            // Plug-ins get the proxy when they load.
            model.load_plugins();
        }
        model.pl_dirty += 1;
        false
    }
}

// --- Plug-ins --------------------------------------------------------------------

/// Turns plug-ins on and off and edits their settings. Ticking a plug-in
/// and pressing OK allows what it says it may do, shown under its name.
pub struct PluginsPanel {
    form: Form<Act>,
    rows: Vec<PluginRow>,
}

struct PluginRow {
    /// Its place in `Model::plugins`.
    plugin: usize,
    id: String,
    /// The permissions it asks for now, stored as allowed when it is on.
    grant: String,
    on: bool,
    fields: Vec<(String, usize)>,
}

impl PluginsPanel {
    fn new(model: &Model) -> PluginsPanel {
        let w = 480;
        let mut form = Form::new(w, 0);
        let mut rows = Vec::new();
        let mut y = 10;
        let line = |form: &mut Form<Act>, y: &mut i32, text: String, dim: bool| {
            form.add(Rect::new(34, *y, w - 44, 20), Widget::Label { text, dim });
            *y += 20;
        };
        if model.plugins.is_empty() {
            let text = format!("No plug-ins in {}", model.paths.plugins_dir().display());
            form.add(Rect::new(10, y, w - 20, 20), Widget::Label { text, dim: true });
            y += 30;
        }
        let config = model.config.plugins();
        for (n, p) in model.plugins.iter().enumerate() {
            let Some(m) = p.manifest() else {
                form.add(Rect::new(14, y, w - 24, 20), Widget::Label { text: p.folder.clone(), dim: false });
                y += 20;
                line(&mut form, &mut y, problem(p).unwrap_or_default(), true);
                y += 8;
                continue;
            };
            let on = matches!(p.state, State::On(_) | State::Failed(..));
            let label = format!("{} {}", m.name, m.version);
            form.add(Rect::new(14, y, w - 24, 20), Widget::Check { label, checked: on, action: Act::Plugin(n) });
            y += 22;
            if !m.description.is_empty() {
                line(&mut form, &mut y, m.description.clone(), true);
            }
            let grant = m.permissions.grant();
            let allowed = config.grant(&m.id);
            let more = if !allowed.is_empty() && allowed != grant { " (more than you allowed)" } else { "" };
            line(&mut form, &mut y, format!("It {}{more}.", m.permissions.describe()), false);
            if let Some(text) = problem(p) {
                line(&mut form, &mut y, text, true);
            }
            let mut fields = Vec::new();
            for s in &m.settings {
                form.add(Rect::new(34, y + 2, 140, 24), Widget::Label { text: s.label.clone(), dim: false });
                let text = config.value(&m.id, &s.key).unwrap_or(&s.default).to_string();
                let cursor = text.chars().count();
                fields.push((s.key.clone(), form.add(Rect::new(176, y, w - 186, 24), Widget::Input { text, cursor })));
                y += 28;
            }
            rows.push(PluginRow { plugin: n, id: m.id.clone(), grant, on, fields });
            y += 8;
        }
        button(&mut form, w - 2 * BUTTON.0 - 16, y, "OK", Act::Ok, true);
        button(&mut form, w - BUTTON.0 - 10, y, "Cancel", Act::Close, false);
        form.h = y + BUTTON.1 + 10;
        PluginsPanel { form, rows }
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Toggled(Act::Plugin(n), on) => {
                if let Some(row) = self.rows.iter_mut().find(|r| r.plugin == n) {
                    row.on = on;
                }
                true
            }
            Event::Clicked(Act::Ok) => {
                self.apply(model);
                false
            }
            Event::Clicked(_) | Event::Cancel => false,
            _ => true,
        }
    }

    /// Stores what is ticked and typed, and loads the plug-ins anew.
    fn apply(&self, model: &mut Model) {
        for row in &self.rows {
            model.config.set_plugin_grant(&row.id, if row.on { &row.grant } else { "" });
            for (key, i) in &row.fields {
                model.config.set_plugin_value(&row.id, key, self.form.text(*i));
            }
        }
        model.load_plugins();
    }
}

/// Why a plug-in is not working, in words.
fn problem(p: &Installed) -> Option<String> {
    match &p.state {
        State::Broken(e) => Some(format!("Broken: {e}")),
        State::Failed(_, e) => Some(format!("Did not load: {e}")),
        State::On(plugin) if plugin.is_off() => Some("Stopped after failing; OK starts it again.".into()),
        _ => None,
    }
}

// --- Skin Museum -----------------------------------------------------------------

/// Browse and search skins.webamp.org, preview one, download and use it.
pub struct SkinMuseum {
    form: Form<Act>,
    search: usize,
    list: usize,
    shot: usize,
    status: usize,
    /// The search the list shows (empty: the museum's own order).
    query: String,
    entries: Vec<museum::Entry>,
    /// A page request is under way.
    loading: bool,
    /// The last page was full, so there may be more.
    more: bool,
    shots: HashMap<String, Bitmap>,
    /// Requests for `app.rs` to run in the background.
    jobs: Vec<Job>,
}

impl SkinMuseum {
    const CREDIT: &'static str = "Skins from skins.webamp.org belong to their authors.";

    fn new() -> SkinMuseum {
        let (list_w, shot_w, shot_h) = (260, 275, 348);
        let w = 10 + list_w + 10 + shot_w + 10;
        let mut form = Form::new(w, 0);
        let search = form.add(Rect::new(10, 10, list_w - 80, 24), Widget::Input { text: String::new(), cursor: 0 });
        button(&mut form, 10 + list_w - 74, 10, "Search", Act::Search, true);
        let list_h = 4 + 16 * ROW_H;
        let list = form.add(Rect::new(10, 42, list_w, list_h), Widget::List { rows: vec![], selected: None, scroll: 0 });
        button(&mut form, 10, 42 + list_h + 8, "More", Act::More, false);
        let shot = form.add(Rect::new(10 + list_w + 10, 10, shot_w, shot_h), Widget::Image { image: None, note: String::new() });
        let y = 10 + shot_h + 14;
        let status = form.add(Rect::new(10, y, w - 2 * BUTTON.0 - 36, BUTTON.1), Widget::Label { text: Self::CREDIT.into(), dim: true });
        form.add(
            Rect::new(w - 2 * BUTTON.0 - 56, y, BUTTON.0 + 40, BUTTON.1),
            Widget::Button { label: "Download & use".into(), action: Act::Ok, default: false },
        );
        button(&mut form, w - BUTTON.0 - 10, y, "Close", Act::Close, false);
        form.h = y + BUTTON.1 + 10;
        let mut p = SkinMuseum {
            form,
            search,
            list,
            shot,
            status,
            query: String::new(),
            entries: vec![],
            loading: false,
            more: false,
            shots: HashMap::new(),
            jobs: vec![],
        };
        p.load(String::new());
        p
    }

    /// Starts a new list for `query`.
    fn load(&mut self, query: String) {
        self.entries.clear();
        self.form.set_rows(self.list, vec![], None);
        self.form.set_image(self.shot, None, "");
        self.query = query;
        self.request_page();
    }

    fn request_page(&mut self) {
        self.loading = true;
        self.form.set_label(self.status, "Loading…".into());
        self.jobs.push(Job::Page { query: self.query.clone(), offset: self.entries.len() });
    }

    fn selected(&self) -> Option<&museum::Entry> {
        self.form.selected(self.list).and_then(|i| self.entries.get(i))
    }

    /// Shows the selected skin's screenshot, fetching it the first time.
    fn show_selected(&mut self) {
        let Some(md5) = self.selected().map(|e| e.md5.clone()) else { return };
        match self.shots.get(&md5) {
            Some(img) => self.form.set_image(self.shot, Some(img.clone()), ""),
            None => {
                self.form.set_image(self.shot, None, "Loading preview…");
                self.jobs.push(Job::Screenshot { md5 });
            }
        }
    }

    fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match ev {
            Event::Clicked(Act::Search) => self.load(self.form.text(self.search).trim().to_string()),
            Event::Clicked(Act::More) if self.more && !self.loading => self.request_page(),
            Event::Selected => self.show_selected(),
            Event::Clicked(Act::Ok) | Event::Activated => {
                if let Some(entry) = self.selected().cloned() {
                    self.form.set_label(self.status, format!("Downloading {}…", entry.name()));
                    self.jobs.push(Job::Download { entry, dir: model.paths.skins_dir() });
                }
            }
            Event::Clicked(Act::Close) | Event::Cancel => return false,
            _ => {}
        }
        true
    }

    fn fetched(&mut self, f: Fetched, model: &mut Model) {
        match f {
            Fetched::Page { query, offset, result } => self.page_fetched(&query, offset, result),
            Fetched::Screenshot { md5, result } => self.screenshot_fetched(md5, result),
            Fetched::Download { name, result } => {
                let note = match result {
                    Ok(path) => {
                        model.set_skin(Some(path));
                        format!("Using {name}.")
                    }
                    Err(e) => format!("{name}: {e}"),
                };
                self.form.set_label(self.status, note);
            }
        }
    }

    fn page_fetched(&mut self, query: &str, offset: usize, result: Result<Vec<museum::Entry>, String>) {
        // Answers to an older search, or a page already shown, are dropped.
        if query != self.query || offset != self.entries.len() {
            return;
        }
        self.loading = false;
        match result {
            Ok(page) => self.add_page(page),
            Err(e) => self.form.set_label(self.status, format!("Museum: {e}")),
        }
    }

    fn screenshot_fetched(&mut self, md5: String, result: Result<Bitmap, String>) {
        let showing = self.selected().is_some_and(|e| e.md5 == md5);
        match result {
            Ok(img) => {
                self.shots.insert(md5, img);
                if showing {
                    self.show_selected();
                }
            }
            Err(e) if showing => self.form.set_image(self.shot, None, &format!("No preview: {e}")),
            Err(_) => {}
        }
    }

    fn add_page(&mut self, page: Vec<museum::Entry>) {
        self.more = page.len() >= museum::PAGE;
        let first = self.entries.is_empty();
        let names: Vec<String> = page.iter().map(museum::Entry::name).collect();
        self.entries.extend(page);
        if first {
            self.form.set_rows(self.list, names, Some(0));
            self.show_selected();
        } else {
            self.form.push_rows(self.list, names);
        }
        let note = match self.entries.len() {
            0 => "Nothing found.".to_string(),
            n => format!("{n} skins. {}", Self::CREDIT),
        };
        self.form.set_label(self.status, note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Key;
    use popamp_core::plformats::Item;

    fn songs(titles: &[&str]) -> Model {
        let mut m = Model::for_tests();
        let items = titles.iter().enumerate().map(|(i, t)| Item {
            location: format!("/music/{i}.mp3"),
            title: Some(t.to_string()),
            length_ms: None,
        });
        m.on_expanded(items.collect(), crate::model::AddMode::Append);
        m
    }

    fn type_text(p: &mut Panel, m: &mut Model, s: &str) -> bool {
        s.chars().all(|c| {
            let ev = p.form_mut().key(Key::Char(c));
            p.handle(ev, m)
        })
    }

    fn press(p: &mut Panel, m: &mut Model, k: Key) -> bool {
        let ev = p.form_mut().key(k);
        p.handle(ev, m)
    }

    fn rows(p: &Panel) -> Vec<String> {
        let Panel::Jump(j) = p else { panic!("not jump to file") };
        match j.form.widget(j.list) {
            Widget::List { rows, .. } => rows.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn search_needs_every_word_in_any_case() {
        let titles = ["Pink Floyd - Time", "Floyd Cramer - Last Date", "Time After Time"];
        assert_eq!(search(titles.into_iter(), "time"), [0, 2]);
        assert_eq!(search(titles.into_iter(), "FLOYD time"), [0]);
        assert_eq!(search(titles.into_iter(), "  "), [0, 1, 2]);
        assert!(search(titles.into_iter(), "jazz").is_empty());
    }

    #[test]
    fn jump_to_file_filters_and_plays() {
        let mut m = songs(&["Alpha", "Beta", "Gamma", "Alphabet"]);
        let mut p = Panel::open(PanelKind::JumpToFile, &m);
        assert_eq!(rows(&p).len(), 4);
        assert!(type_text(&mut p, &mut m, "alp"));
        assert_eq!(rows(&p), ["1. Alpha", "4. Alphabet"]);
        press(&mut p, &mut m, Key::Down);
        assert!(!press(&mut p, &mut m, Key::Enter), "jumping closes the window");
        assert_eq!(m.playlist.current(), Some(3));
    }

    #[test]
    fn jump_to_file_follows_the_playlist() {
        let mut m = songs(&["Alpha", "Beta"]);
        let mut p = Panel::open(PanelKind::JumpToFile, &m);
        type_text(&mut p, &mut m, "a");
        m.on_expanded(
            vec![Item { location: "/x.mp3".into(), title: Some("Delta".into()), length_ms: None }],
            crate::model::AddMode::Append,
        );
        p.playlist_changed(&m);
        assert_eq!(rows(&p), ["1. Alpha", "2. Beta", "3. Delta"]);
        // Nothing matches: Enter does nothing, Escape closes.
        type_text(&mut p, &mut m, "zzz");
        assert!(press(&mut p, &mut m, Key::Enter));
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.playlist.current(), None);
    }

    #[test]
    fn file_info_lists_the_selected_entry() {
        let mut m = songs(&["Alpha", "Beta"]);
        m.playlist.select_only(1);
        let p = Panel::open(PanelKind::FileInfo, &m);
        let labels: Vec<String> = (0..4)
            .map(|i| match p.form().widget(i) {
                Widget::Label { text, .. } => text.clone(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(labels, ["Location", "/music/1.mp3", "Title", "Beta"]);
        let mut p = p;
        assert!(!press(&mut p, &mut m, Key::Enter));
    }

    /// Where the check box for `flag` is.
    fn check_box(p: &Panel, flag: Flag) -> Rect {
        let Panel::Prefs(pr) = p else { panic!("not preferences") };
        let i = (0..).find(|&i| matches!(pr.form.widget(i), Widget::Check { action: Act::Flag(f), .. } if *f == flag)).unwrap();
        pr.form.rect(i)
    }

    /// A model with plug-in folders holding `manifests`; their modules are
    /// not WebAssembly, so a plug-in that is allowed fails to load.
    fn with_plugins(manifests: &[(&str, &str)], allowed: &[(&str, &str)]) -> Model {
        let mut m = Model::for_tests();
        let root = m.paths.plugins_dir();
        for (id, text) in manifests {
            std::fs::create_dir_all(root.join(id)).unwrap();
            std::fs::write(root.join(id).join("plugin.toml"), text).unwrap();
            std::fs::write(root.join(id).join("plugin.wasm"), b"").unwrap();
        }
        for (id, grant) in allowed {
            m.config.set_plugin_grant(id, grant);
        }
        m.plugins = popamp_plugins::installed::load_all(&root, &m.config.plugins(), &Default::default());
        m
    }

    const YT: &str = "id = \"yt\"\nname = \"YT\"\nversion = \"2\"\napi = \"1.0\"\nkind = \"source\"\ndescription = \"Videos.\"\n[source]\nhosts = [\"x.invalid\"]\n[permissions]\nexec = [\"yt-dlp\"]\n[[setting]]\nkey = \"format\"\nlabel = \"Format\"\ntype = \"text\"\ndefault = \"best\"\n";

    fn labels(p: &Panel) -> Vec<String> {
        let f = p.form();
        (0..f.count())
            .filter_map(|i| match f.widget(i) {
                Widget::Label { text, .. } => Some(text.clone()),
                Widget::Check { label, checked, .. } => Some(format!("[{}] {label}", if *checked { "x" } else { " " })),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ticking_a_plugin_and_pressing_ok_allows_what_it_says_and_keeps_its_settings() {
        let mut m = with_plugins(&[("yt", YT), ("bad", "id = 1")], &[]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[ ] YT 2".to_string()), "{shown:?}");
        assert!(shown.contains(&"It runs yt-dlp with your rights.".to_string()), "{shown:?}");
        assert!(shown.iter().any(|l| l.starts_with("Broken: plugin.toml:")), "{shown:?}");
        let f = p.form();
        let check = (0..f.count()).find(|&i| matches!(f.widget(i), Widget::Check { .. })).unwrap();
        let input = (0..f.count()).find(|&i| matches!(f.widget(i), Widget::Input { .. })).unwrap();
        let (check, input) = (f.rect(check), f.rect(input));
        click(&mut p, &mut m, check);
        click(&mut p, &mut m, input);
        type_text(&mut p, &mut m, "!");
        assert!(!press(&mut p, &mut m, Key::Enter), "OK closes");
        assert_eq!(m.config.plugins().grant("yt"), "exec=yt-dlp;net=");
        assert_eq!(m.config.plugins().value("yt", "format"), Some("best!"));
    }

    #[test]
    fn a_plugin_asking_for_more_than_allowed_says_so_and_cancel_changes_nothing() {
        let mut m = with_plugins(&[("yt", YT)], &[("yt", "exec=;net=")]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[ ] YT 2".to_string()), "{shown:?}");
        assert!(shown.contains(&"It runs yt-dlp with your rights (more than you allowed).".to_string()), "{shown:?}");
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.config.plugins().grant("yt"), "exec=;net=");
    }

    #[test]
    fn an_allowed_plugin_that_did_not_load_is_shown_ticked_with_the_reason() {
        let m = with_plugins(&[("yt", YT)], &[("yt", "exec=yt-dlp;net=")]);
        let shown = labels(&Panel::open(PanelKind::Plugins, &m));
        assert!(shown.contains(&"[x] YT 2".to_string()), "{shown:?}");
        assert!(shown.iter().any(|l| l.starts_with("Did not load: cannot compile")), "{shown:?}");
        let empty = labels(&Panel::open(PanelKind::Plugins, &Model::for_tests()));
        assert!(empty[0].starts_with("No plug-ins in "), "{empty:?}");
    }

    fn click(p: &mut Panel, m: &mut Model, r: Rect) -> bool {
        let ev = p.form_mut().mouse_down(r.x + 2, r.y + 2, false);
        p.handle(ev, m)
    }

    #[test]
    fn preference_check_boxes_apply_at_once() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        assert!(m.settings().snap && m.settings().pladv && !m.settings().aot);
        let r = check_box(&p, Flag::Snap);
        click(&mut p, &mut m, r);
        let r = check_box(&p, Flag::AutoAdvance);
        click(&mut p, &mut m, r);
        let r = check_box(&p, Flag::AlwaysOnTop);
        click(&mut p, &mut m, r);
        assert!(!m.settings().snap && !m.settings().pladv && m.settings().aot);
        assert!(m.ui.iter().any(|r| matches!(r, UiReq::AlwaysOnTop)), "the windows are told");
    }

    fn field(p: &Panel, f: Field) -> usize {
        let Panel::Prefs(pr) = p else { panic!("not preferences") };
        pr.fields.iter().find(|(g, _)| *g == f).unwrap().1
    }

    fn retype(p: &mut Panel, m: &mut Model, f: Field, text: &str) {
        let r = p.form().rect(field(p, f));
        click(p, m, r);
        for _ in 0..100 {
            press(p, m, Key::Backspace);
        }
        type_text(p, m, text);
    }

    #[test]
    fn typed_preferences_apply_on_ok_only_when_valid() {
        let mut m = songs(&["Alpha"]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        retype(&mut p, &mut m, Field::FontSize, "14");
        retype(&mut p, &mut m, Field::FadeMs, "soon");
        assert!(press(&mut p, &mut m, Key::Enter), "a bad number keeps the window open");
        assert_eq!(m.settings().pe_fontsize, 10);
        let status = match &p {
            Panel::Prefs(pr) => pr.form.widget(pr.status).clone(),
            _ => unreachable!(),
        };
        assert!(matches!(status, Widget::Label { text, .. } if text.contains("Fade-out")));
        retype(&mut p, &mut m, Field::FadeMs, "500");
        retype(&mut p, &mut m, Field::TitleFormat, "%title%!");
        assert!(!press(&mut p, &mut m, Key::Enter));
        assert_eq!((m.settings().pe_fontsize, m.settings().fade_ms), (14, 500));
        assert_eq!(m.settings().titlefmt, "%title%!");
    }

    #[test]
    fn cancel_forgets_typed_preferences() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        retype(&mut p, &mut m, Field::FontSize, "30");
        assert!(!press(&mut p, &mut m, Key::Escape));
        assert_eq!(m.settings().pe_fontsize, 10);
    }

    #[test]
    fn numbers_are_kept_in_range() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::Preferences, &m);
        retype(&mut p, &mut m, Field::FontSize, "999");
        press(&mut p, &mut m, Key::Enter);
        assert_eq!(m.settings().pe_fontsize, 40);
    }

    fn museum_entry(n: u8) -> museum::Entry {
        museum::Entry { md5: format!("{n:032x}"), filename: format!("Skin {n}.wsz") }
    }

    fn museum_page(query: &str, offset: usize, n: usize) -> Fetched {
        let result = Ok((0..n).map(|i| museum_entry((offset + i) as u8)).collect());
        Fetched::Page { query: query.into(), offset, result }
    }

    fn museum(p: &Panel) -> &SkinMuseum {
        match p {
            Panel::Museum(m) => m,
            _ => panic!("not the museum"),
        }
    }

    #[test]
    fn museum_lists_pages_and_previews_the_selection() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::SkinMuseum, &m);
        assert!(matches!(&p.take_jobs()[..], [Job::Page { query, offset: 0 }] if query.is_empty()));
        p.fetched(museum_page("", 0, museum::PAGE), &mut m);
        assert_eq!(museum(&p).form.selected(museum(&p).list), Some(0));
        let md5_0 = museum_entry(0).md5;
        assert!(matches!(&p.take_jobs()[..], [Job::Screenshot { md5 }] if *md5 == md5_0));
        // More appends the next page; a late copy of the first is ignored.
        let ev = Event::Clicked(Act::More);
        p.handle(ev, &mut m);
        assert!(matches!(&p.take_jobs()[..], [Job::Page { offset, .. }] if *offset == museum::PAGE));
        p.fetched(museum_page("", 0, 3), &mut m);
        p.fetched(museum_page("", museum::PAGE, 2), &mut m);
        assert_eq!(museum(&p).entries.len(), museum::PAGE + 2);
        p.handle(Event::Clicked(Act::More), &mut m);
        assert!(p.take_jobs().is_empty(), "a short page was the last");
        // A preview is fetched once, then shown from the cache.
        p.fetched(Fetched::Screenshot { md5: md5_0.clone(), result: Ok(Bitmap::new(2, 2, 0)) }, &mut m);
        assert!(matches!(museum(&p).form.widget(museum(&p).shot), Widget::Image { image: Some(_), .. }));
        press(&mut p, &mut m, Key::Down);
        assert_eq!(p.take_jobs().len(), 1);
        press(&mut p, &mut m, Key::Up);
        assert!(p.take_jobs().is_empty());
    }

    #[test]
    fn museum_search_replaces_the_list() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::SkinMuseum, &m);
        p.fetched(museum_page("", 0, 5), &mut m);
        type_text(&mut p, &mut m, "bento");
        press(&mut p, &mut m, Key::Enter);
        assert!(matches!(&p.take_jobs()[..], [_, _, Job::Page { query, offset: 0 }] if query == "bento"));
        p.fetched(museum_page("", 5, 5), &mut m);
        assert!(museum(&p).entries.is_empty(), "answer to the old list");
        p.fetched(Fetched::Page { query: "bento".into(), offset: 0, result: Ok(vec![]) }, &mut m);
        let status = museum(&p).form.widget(museum(&p).status).clone();
        assert!(matches!(status, Widget::Label { text, .. } if text == "Nothing found."));
    }

    #[test]
    fn museum_downloads_into_the_skins_folder() {
        let mut m = songs(&[]);
        let mut p = Panel::open(PanelKind::SkinMuseum, &m);
        p.fetched(museum_page("", 0, 2), &mut m);
        p.take_jobs();
        p.handle(Event::Activated, &mut m);
        let dir = m.paths.skins_dir();
        assert!(matches!(&p.take_jobs()[..], [Job::Download { entry, dir: d }] if entry.md5 == museum_entry(0).md5 && *d == dir));
        let failed = Fetched::Download { name: "Skin 0".into(), result: Err("not a usable skin".into()) };
        p.fetched(failed, &mut m);
        let status = museum(&p).form.widget(museum(&p).status).clone();
        assert!(matches!(status, Widget::Label { text, .. } if text == "Skin 0: not a usable skin"));
        assert!(!press(&mut p, &mut m, Key::Escape));
    }
}
