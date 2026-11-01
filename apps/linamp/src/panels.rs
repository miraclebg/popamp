//! Linamp's own dialog windows: Jump to file, File info and Preferences.
//! Each is a [`Form`] plus the logic that turns its events into changes to
//! the [`Model`]; the windows themselves live in `app.rs`.

use crate::model::{Model, UiReq};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON, ROW_H};
use linamp_core::config::Settings;
use linamp_core::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    JumpToFile,
    FileInfo,
    Preferences,
}

/// What a button or check box in a panel stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Ok,
    Close,
    Flag(Flag),
}

pub enum Panel {
    Jump(JumpToFile),
    Info(FileInfo),
    Prefs(Preferences),
}

impl Panel {
    pub fn open(kind: PanelKind, model: &Model) -> Panel {
        match kind {
            PanelKind::JumpToFile => Panel::Jump(JumpToFile::new(model)),
            PanelKind::FileInfo => Panel::Info(FileInfo::new(model)),
            PanelKind::Preferences => Panel::Prefs(Preferences::new(model.settings())),
        }
    }

    pub fn kind(&self) -> PanelKind {
        match self {
            Panel::Jump(_) => PanelKind::JumpToFile,
            Panel::Info(_) => PanelKind::FileInfo,
            Panel::Prefs(_) => PanelKind::Preferences,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Panel::Jump(_) => "Jump to file",
            Panel::Info(_) => "File info",
            Panel::Prefs(_) => "Linamp Preferences",
        }
    }

    pub fn form(&self) -> &Form<Act> {
        match self {
            Panel::Jump(p) => &p.form,
            Panel::Info(p) => &p.form,
            Panel::Prefs(p) => &p.form,
        }
    }

    pub fn form_mut(&mut self) -> &mut Form<Act> {
        match self {
            Panel::Jump(p) => &mut p.form,
            Panel::Info(p) => &mut p.form,
            Panel::Prefs(p) => &mut p.form,
        }
    }

    /// Acts on what happened in the form. Returns `false` when the panel
    /// should close.
    pub fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
        match self {
            Panel::Jump(p) => p.handle(ev, model),
            Panel::Info(_) => !matches!(ev, Event::Clicked(_) | Event::Cancel),
            Panel::Prefs(p) => p.handle(ev, model),
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
        for &(f, i) in &self.fields {
            f.apply(model, self.form.text(i));
        }
        model.settings_mut().sanitize();
        model.push_engine_settings();
        model.pl_dirty += 1;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Key;
    use linamp_core::plformats::Item;

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
}
