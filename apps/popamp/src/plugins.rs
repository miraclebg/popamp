//! Plug-ins, in a build with the `plugins` feature: loading them, and the
//! Plug-ins window that turns them on and off.

use crate::model::{AppEvent, Model};
use crate::panels::{button, Act};
use crate::widgets::{Event, Form, Rect, Widget, BUTTON};
use popamp_audio::dsp::Effect;
use popamp_audio::EngineCmd;
use popamp_core::config::PluginConfig;
use popamp_plugins::installed::{self, Installed, State};
use popamp_plugins::manifest::Manifest;

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
        let (roots, config, cx, events) = (self.paths.plugins_dirs(), self.config.plugins(), self.source_context(), self.events.clone());
        let own = [self.paths.config_dir.clone(), self.paths.data_dir.clone()];
        let _ = std::thread::Builder::new().name("popamp-plugins".into()).spawn(move || {
            let list = installed::load_all(&roots, &config, &cx, &own);
            let effects = installed::effects(&list);
            events.send(AppEvent::Plugins(n, list, effects));
        });
    }

    /// Puts the source plug-ins that are on in front of the built-in
    /// sources, and gives the engine the dsp ones.
    pub fn on_plugins(&mut self, n: u64, list: Vec<Installed>, effects: Vec<Box<dyn Effect>>) {
        if n != self.plugins.loads {
            return;
        }
        for p in list.iter().filter_map(|i| if let State::On(p) = &i.state { Some(p) } else { None }) {
            let (events, id) = (self.events.clone(), p.manifest().id.clone());
            p.when_stopped(Box::new(move |why| {
                events.send(AppEvent::PluginStopped(id.clone(), why.to_string()));
            }));
        }
        self.sources.set_plugins(installed::sources(&list));
        self.engine.send(EngineCmd::Effects(effects));
        self.plugins.list = list;
    }

    /// Says in the title display that a plug-in stopped, and why; the
    /// Plug-ins window says it too.
    /// The visual plug-ins that are on, to open from the menu.
    #[cfg(feature = "visualizers")]
    pub fn visualizer_items(&self) -> Vec<crate::menu::MenuItem> {
        use crate::menu::{MenuAction, MenuItem};
        let plugins = installed::visuals(&self.plugins.list);
        let open = plugins.iter().map(|p| MenuItem::action(&p.manifest().name, "", MenuAction::Visualizer(p.manifest().id.clone())));
        plugins.first().map(|_| MenuItem::sep()).into_iter().chain(open).collect()
    }

    /// The visual plug-in with this id, if it is on.
    #[cfg(feature = "visualizers")]
    pub fn visual_plugin(&self, id: &str) -> Option<std::sync::Arc<popamp_plugins::host::Plugin>> {
        installed::visuals(&self.plugins.list).into_iter().find(|p| p.manifest().id == id)
    }

    pub fn on_plugin_stopped(&mut self, id: &str, why: &str) {
        let name = self.plugins.list.iter().filter_map(Installed::manifest).find(|m| m.id == id).map_or(id, |m| m.name.as_str());
        log::warn!("plug-in {id} stopped: {why}");
        self.flash(format!("PLUG-IN {} STOPPED: {why}", name.to_uppercase()));
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
    /// The hosts its manifest lists, and whether each is ticked.
    hosts: Vec<(String, bool)>,
    /// The field with the folders it may read, if it reads files.
    folders: Option<usize>,
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
            for command in &m.permissions.exec {
                for text in wrapped(&command.text()).into_iter().chain(command.limits()) {
                    line(&mut form, &mut y, text, true);
                }
            }
            if let Some(text) = problem(p) {
                line(&mut form, &mut y, text, true);
            }
            let (hosts, folders) = permissions(&mut form, &mut y, n, m, &config);
            let fields = settings_fields(&mut form, &mut y, m, &config);
            rows.push(PluginRow { plugin: n, id: m.id.clone(), grant, on, hosts, folders, fields });
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
            Event::Toggled(Act::PluginHost(n, host), on) => {
                if let Some(row) = self.rows.iter_mut().find(|r| r.plugin == n) {
                    row.hosts[host].1 = on;
                }
                true
            }
            Event::Clicked(Act::PluginJournal(n)) => {
                open_journal(model, n);
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
            let (off, on) = installed::stored_addresses(&row.hosts);
            model.config.set_plugin_limit(&row.id, installed::HOSTS_OFF, &off);
            model.config.set_plugin_limit(&row.id, installed::HOSTS_ON, &on);
            if let Some(i) = row.folders {
                model.config.set_plugin_limit(&row.id, installed::FOLDERS, self.form.text(i));
            }
            for (key, i) in &row.fields {
                model.config.set_plugin_value(&row.id, key, self.form.text(*i));
            }
        }
        model.load_plugins();
    }
}

/// The fields of the plug-in's own settings, by key.
fn settings_fields(form: &mut Form<Act>, y: &mut i32, m: &Manifest, config: &PluginConfig) -> Vec<(String, usize)> {
    let w = form.w;
    let mut fields = Vec::new();
    for s in &m.settings {
        form.add(Rect::new(34, *y + 2, 140, 24), Widget::Label { text: s.label.clone(), dim: false });
        let text = config.value(&m.id, &s.key).unwrap_or(&s.default).to_string();
        let cursor = text.chars().count();
        fields.push((s.key.clone(), form.add(Rect::new(176, *y, w - 186, 24), Widget::Input { text, cursor })));
        *y += 28;
    }
    fields
}

/// The block where the user narrows what plug-in number `n` may do: a
/// check box per host, the folders it may read, and its journal. Comes
/// before the plug-in's own settings. Returns the hosts and the folder field.
fn permissions(form: &mut Form<Act>, y: &mut i32, n: usize, m: &Manifest, config: &PluginConfig) -> (Vec<(String, bool)>, Option<usize>) {
    let w = form.w;
    let asks = &m.permissions;
    if asks.exec.is_empty() && asks.net.is_empty() && asks.files.is_none() {
        return (Vec::new(), None);
    }
    form.add(Rect::new(34, *y, w - 44, 20), Widget::Label { text: "Permissions".into(), dim: false });
    *y += 22;
    let hosts = installed::addresses(m, config);
    for (i, (address, open)) in hosts.iter().enumerate() {
        let label = match installed::caution(address) {
            Some(why) => format!("Connect to {address} ({why})"),
            None => format!("Connect to {address}"),
        };
        form.add(Rect::new(34, *y, w - 44, 20), Widget::Check { label, checked: *open, action: Act::PluginHost(n, i) });
        *y += 22;
    }
    let folders = asks.files.map(|_| {
        let hint = "Folders it may read, separated by ; (~ is your whole home):";
        form.add(Rect::new(34, *y, w - 44, 20), Widget::Label { text: hint.into(), dim: true });
        *y += 20;
        let text = installed::folders_text(m, config, std::env::home_dir().as_deref());
        let cursor = text.chars().count();
        let field = form.add(Rect::new(34, *y, w - 44, 24), Widget::Input { text, cursor });
        *y += 28;
        form.add(
            Rect::new(34, *y, w - 44, 20),
            Widget::Label { text: "Hidden folders and the player's own stay closed.".into(), dim: true },
        );
        *y += 22;
        field
    });
    button(form, 34, *y, "Journal", Act::PluginJournal(n), false);
    *y += BUTTON.1 + 8;
    (hosts, folders)
}

/// Shows what plug-in number `n` asked for; an empty file if nothing yet.
fn open_journal(model: &mut Model, n: usize) {
    let Some(m) = model.plugins.list.get(n).and_then(Installed::manifest) else { return };
    let Some(path) = installed::journal_path(&model.paths.plugins_dirs(), &m.id) else { return };
    let made = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(&path));
    let viewer = if cfg!(windows) { "notepad" } else { "xdg-open" };
    if let Err(e) = made.and_then(|_| std::process::Command::new(viewer).arg(&path).spawn()) {
        log::warn!("{}: {e}", path.display());
        model.flash(format!("CANNOT OPEN JOURNAL: {}", path.display()));
    }
}

/// A command in lines that fit the window, broken between words.
fn wrapped(text: &str) -> Vec<String> {
    const COLUMNS: usize = 64;
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let last = lines.last_mut().expect("starts with one");
        if !last.is_empty() && last.len() + 1 + word.len() > COLUMNS {
            lines.push(word.to_string());
        } else {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        }
    }
    lines
}

/// Why a plug-in is not working, in words.
fn problem(p: &Installed) -> Option<String> {
    match &p.state {
        State::Broken(e) => Some(format!("Broken: {e}")),
        State::Failed(_, e) => Some(format!("Did not load: {e}")),
        State::On(plugin) => plugin.stopped_because().map(|why| format!("Stopped after failing: {why}. OK starts it again.")),
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
        m.plugins.list = installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &Default::default(), &[]);
        m
    }

    const YT: &str = "id = \"yt\"\nname = \"YT\"\nversion = \"2\"\napi = \"1.0\"\nkind = \"source\"\ndescription = \"Videos.\"\n[source]\nhosts = [\"x.invalid\"]\n[[permissions.exec]]\nid = \"get\"\nprogram = \"yt-dlp\"\nargs = [\"-j\", \"{url}\"]\n[permissions.exec.vars]\nurl = { type = \"url\", hosts = [\"x.invalid\"] }\n[[setting]]\nkey = \"format\"\nlabel = \"Format\"\ntype = \"text\"\ndefault = \"best\"\n";

    fn grant_of(manifest: &str) -> String {
        popamp_plugins::manifest::Manifest::parse(manifest).unwrap().permissions.grant()
    }

    #[test]
    fn a_long_command_is_broken_between_words() {
        let lines = wrapped(&"word ".repeat(30));
        assert!(lines.len() > 1 && lines.iter().all(|l| l.len() <= 64), "{lines:?}");
        assert_eq!(lines.join(" ").trim_end(), "word ".repeat(30).trim_end());
        assert_eq!(wrapped("short one"), ["short one"]);
    }

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
        assert!(shown.contains(&"yt-dlp -j {url}".to_string()), "the user reads the command: {shown:?}");
        assert!(shown.contains(&"{url} only on x.invalid".to_string()), "{shown:?}");
        assert!(shown.contains(&"Permissions".to_string()), "{shown:?}");
        assert!(shown.iter().any(|l| l.starts_with("Broken: plugin.toml:")), "{shown:?}");
        let f = p.form();
        let check = (0..f.count()).find(|&i| matches!(f.widget(i), Widget::Check { .. })).unwrap();
        let input = (0..f.count()).find(|&i| matches!(f.widget(i), Widget::Input { .. })).unwrap();
        let (check, input) = (f.rect(check), f.rect(input));
        click(&mut p, &mut m, check);
        click(&mut p, &mut m, input);
        type_text(&mut p, &mut m, "!");
        assert!(!press(&mut p, &mut m, Key::Enter), "OK closes");
        assert_eq!(m.config.plugins().grant("yt"), grant_of(YT));
        assert_eq!(m.config.plugins().value("yt", "format"), Some("best!"));
    }

    const NET: &str = "id = \"n\"\nname = \"N\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"source\"\n[source]\nhosts = [\"x.invalid\"]\n[permissions]\nnet = [\"a.invalid\", \"b.invalid\"]\nfiles = \"read\"\n[[setting]]\nkey = \"own\"\nlabel = \"Own\"\ntype = \"text\"\ndefault = \"d\"\n";

    fn inputs(p: &Panel) -> Vec<usize> {
        let f = p.form();
        (0..f.count()).filter(|&i| matches!(f.widget(i), Widget::Input { .. })).collect()
    }

    fn check_labelled(p: &Panel, label: &str) -> Rect {
        let f = p.form();
        f.rect((0..f.count()).find(|&i| matches!(f.widget(i), Widget::Check { label: l, .. } if l == label)).unwrap())
    }

    #[test]
    fn hosts_can_be_unticked_and_folders_chosen_above_the_plugins_own_settings() {
        let mut m = with_plugins(&[("n", NET)], &[]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[x] Connect to a.invalid".to_string()), "{shown:?}");
        assert!(shown.contains(&"[x] Connect to b.invalid".to_string()), "{shown:?}");
        let (folders, own) = (inputs(&p)[0], inputs(&p)[1]);
        let f = p.form();
        assert!(f.rect(folders).y < f.rect(own).y, "the permissions come first");
        assert_eq!(f.text(own), "d");
        let (host, field) = (check_labelled(&p, "Connect to b.invalid"), f.rect(folders));
        click(&mut p, &mut m, host);
        click(&mut p, &mut m, field);
        type_text(&mut p, &mut m, ";/mnt");
        assert!(!press(&mut p, &mut m, Key::Enter));
        let config = m.config.plugins();
        assert_eq!(config.limit("n", installed::HOSTS_OFF), Some("b.invalid"));
        assert!(config.limit("n", installed::FOLDERS).unwrap().ends_with(";/mnt"));
        assert_eq!(config.value("n", "folders"), None, "not one of its settings");
        let again = Panel::open(PanelKind::Plugins, &m);
        assert!(labels(&again).contains(&"[ ] Connect to b.invalid".to_string()), "kept: {:?}", labels(&again));
    }

    #[test]
    fn an_unusual_port_or_a_local_address_stays_off_until_ticked() {
        let text = NET.replace("\"b.invalid\"", "\"b.invalid:8000\", \"localhost:8000\"");
        let mut m = with_plugins(&[("n", &text)], &[]);
        let mut p = Panel::open(PanelKind::Plugins, &m);
        let shown = labels(&p);
        assert!(shown.contains(&"[x] Connect to a.invalid".to_string()), "{shown:?}");
        assert!(shown.contains(&"[ ] Connect to b.invalid:8000 (unusual port)".to_string()), "{shown:?}");
        assert!(shown.contains(&"[ ] Connect to localhost:8000 (this computer or your network)".to_string()), "{shown:?}");
        let local = check_labelled(&p, "Connect to localhost:8000 (this computer or your network)");
        click(&mut p, &mut m, local);
        assert!(!press(&mut p, &mut m, Key::Enter));
        let config = m.config.plugins();
        assert_eq!(config.limit("n", installed::HOSTS_ON), Some("localhost:8000"));
        assert_eq!(config.limit("n", installed::HOSTS_OFF), Some(""));
        let again = labels(&Panel::open(PanelKind::Plugins, &m));
        assert!(again.contains(&"[x] Connect to localhost:8000 (this computer or your network)".to_string()), "{again:?}");
        assert!(again.contains(&"[ ] Connect to b.invalid:8000 (unusual port)".to_string()), "{again:?}");
    }

    #[test]
    fn a_plugin_that_asks_for_nothing_has_no_permissions_block() {
        let plain = "id = \"p\"\nname = \"P\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"dsp\"\n";
        let m = with_plugins(&[("p", plain)], &[]);
        assert!(!labels(&Panel::open(PanelKind::Plugins, &m)).contains(&"Permissions".to_string()));
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
        let m = with_plugins(&[("yt", YT)], &[("yt", &grant_of(YT))]);
        let shown = labels(&Panel::open(PanelKind::Plugins, &m));
        assert!(shown.contains(&"[x] YT 2".to_string()), "{shown:?}");
        assert!(shown.iter().any(|l| l.starts_with("Did not load: cannot compile")), "{shown:?}");
        let empty = labels(&Panel::open(PanelKind::Plugins, &Model::for_tests()));
        assert!(empty[0].starts_with("No plug-ins in "), "{empty:?}");
    }

    #[test]
    fn a_plugin_that_stops_while_playing_says_why() {
        let mut m = Model::for_tests();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/popamp-plugins/tests/fixtures");
        let dir = m.paths.plugins_dir().join("test-dsp");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-dsp/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        m.config.set_plugin_grant("test-dsp", "exec=;net=");
        m.config.set_plugin_value("test-dsp", "mode", "short");
        let list = installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &m.source_context(), &[]);
        let mut fx = installed::effects(&list);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        fx[0].reset(8_000);
        for _ in 0..popamp_plugins::host::FAILURES_TO_STOP {
            fx[0].process(&mut vec![0.0; 8192]);
        }
        let shown = labels(&Panel::open(PanelKind::Plugins, &m));
        let why = "Stopped after failing: a block of 32768 bytes came back as 4. OK starts it again.";
        assert!(shown.contains(&why.to_string()), "{shown:?}");
        m.on_plugin_stopped("test-dsp", "a block came back short");
        let flashed = m.transient.as_ref().map(|t| t.0.as_str());
        assert_eq!(flashed, Some("PLUG-IN TEST DSP STOPPED: a block came back short"));
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
        let load = |m: &Model| installed::load_all(&m.paths.plugins_dirs(), &m.config.plugins(), &m.source_context(), &[]);
        let list = load(&m);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        assert_eq!(m.sources.of("https://test.invalid/list").id(), "http", "not allowed yet");
        m.config.set_plugin_grant("test", &grant_of(&std::fs::read_to_string(dir.join("plugin.toml")).unwrap()));
        let list = load(&m);
        m.on_plugins(m.plugins.loads, list, Vec::new());
        assert_eq!(m.sources.of("https://test.invalid/list").id(), "test");
        m.add_now(vec!["https://test.invalid/list".into()], AddMode::Append);
        let got: Vec<_> = m.playlist.entries().iter().map(|e| (e.location.clone(), e.file_title.clone())).collect();
        assert_eq!(got[1], ("https://test.invalid/list/2".to_string(), Some("Track 2".to_string())));
        m.on_plugins(m.plugins.loads - 1, Vec::new(), Vec::new());
        assert_eq!(m.plugins.list.len(), 1, "an older answer is dropped");
    }
}
