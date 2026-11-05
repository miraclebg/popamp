//! Plug-ins, in a build with the `plugins` feature: loading them, and the
//! Plug-ins window that turns them on and off.

use crate::model::{AppEvent, Model};
use crate::panels::{button, Act};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON};
use popamp_plugins::installed::{self, Installed, State};

/// The plug-ins in the plug-ins folder and what became of each.
#[derive(Default)]
pub struct Plugins {
    pub list: Vec<Installed>,
    /// Requests to load the plug-ins; only the last one's answer counts.
    loads: u64,
}

impl Model {
    /// Loads the plug-ins the user allowed, on a thread of its own:
    /// compiling a module takes a while.
    pub fn load_plugins(&mut self) {
        self.plugins.loads += 1;
        let n = self.plugins.loads;
        let (root, config, cx, events) = (self.paths.plugins_dir(), self.config.plugins(), self.source_context(), self.events.clone());
        let _ = std::thread::Builder::new().name("popamp-plugins".into()).spawn(move || {
            events.send(AppEvent::Plugins(n, installed::load_all(&root, &config, &cx)));
        });
    }

    /// Puts the plug-ins that are on in front of the built-in sources.
    pub fn on_plugins(&mut self, n: u64, list: Vec<Installed>) {
        if n != self.plugins.loads {
            return;
        }
        self.sources.set_plugins(installed::sources(&list));
        self.plugins.list = list;
    }
}

/// Turns plug-ins on and off and edits their settings. Ticking a plug-in
/// and pressing OK allows what it says it may do, shown under its name.
pub struct PluginsPanel {
    pub(crate) form: Form<Act>,
    rows: Vec<PluginRow>,
}

struct PluginRow {
    /// Its place in `Plugins::list`.
    plugin: usize,
    id: String,
    /// The permissions it asks for now, stored as allowed when it is on.
    grant: String,
    on: bool,
    fields: Vec<(String, usize)>,
}

impl PluginsPanel {
    pub(crate) fn new(model: &Model) -> PluginsPanel {
        let w = 480;
        let mut form = Form::new(w, 0);
        let mut rows = Vec::new();
        let mut y = 10;
        let line = |form: &mut Form<Act>, y: &mut i32, text: String, dim: bool| {
            form.add(Rect::new(34, *y, w - 44, 20), Widget::Label { text, dim });
            *y += 20;
        };
        if model.plugins.list.is_empty() {
            let text = format!("No plug-ins in {}", model.paths.plugins_dir().display());
            form.add(Rect::new(10, y, w - 20, 20), Widget::Label { text, dim: true });
            y += 30;
        }
        let config = model.config.plugins();
        for (n, p) in model.plugins.list.iter().enumerate() {
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

    pub(crate) fn handle(&mut self, ev: Event<Act>, model: &mut Model) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AddMode;
    use crate::panels::tests::{click, press, type_text};
    use crate::panels::{Panel, PanelKind};
    use crate::widgets::Key;
    use std::path::Path;

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
        m.plugins.list = installed::load_all(&root, &m.config.plugins(), &Default::default());
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

    #[test]
    fn an_allowed_plugin_takes_its_links_before_the_built_in_sources() {
        let mut m = Model::for_tests();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-plugins/tests/fixtures/test");
        let dir = m.paths.plugins_dir().join("test");
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["plugin.toml", "plugin.wasm"] {
            std::fs::copy(fixture.join(f), dir.join(f)).unwrap();
        }
        let load = |m: &Model| installed::load_all(&m.paths.plugins_dir(), &m.config.plugins(), &m.source_context());
        let list = load(&m);
        m.on_plugins(m.plugins.loads, list);
        assert_eq!(m.sources.of("https://test.invalid/list").id(), "http", "not allowed yet");
        m.config.set_plugin_grant("test", "exec=sh;net=");
        let list = load(&m);
        m.on_plugins(m.plugins.loads, list);
        assert_eq!(m.sources.of("https://test.invalid/list").id(), "test");
        m.add_now(vec!["https://test.invalid/list".into()], AddMode::Append);
        let got: Vec<_> = m.playlist.entries().iter().map(|e| (e.location.clone(), e.file_title.clone())).collect();
        assert_eq!(got[1], ("https://test.invalid/list/2".to_string(), Some("Track 2".to_string())));
        m.on_plugins(m.plugins.loads - 1, Vec::new());
        assert_eq!(m.plugins.list.len(), 1, "an older answer is dropped");
    }
}
