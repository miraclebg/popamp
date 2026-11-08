//! The plug-ins in the plug-ins folder and what became of each: broken,
//! off, on, or failed to load. A plug-in is on only while the permissions
//! the user allowed are the ones its manifest asks for now, so a new
//! version that asks for more waits to be allowed again.

use crate::command::Command;
use crate::files::{self, Closed};
use crate::host::{Access, Plugin};
use crate::journal::Journal;
use crate::manifest::{Kind, Manifest, Permissions};
use crate::net::NetEntry;
use popamp_audio::dsp::Effect;
use popamp_audio::source::{Context, Source};
use popamp_core::config::PluginConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct Installed {
    /// The folder's name.
    pub folder: String,
    pub dir: PathBuf,
    pub state: State,
}

#[derive(Debug)]
pub enum State {
    /// Its manifest is missing or wrong.
    Broken(String),
    /// Not allowed, or allowed permissions other than it asks for now.
    Off(Manifest),
    On(Arc<Plugin>),
    /// Allowed, but its module did not load.
    Failed(Manifest, String),
}

impl Installed {
    pub fn manifest(&self) -> Option<&Manifest> {
        match &self.state {
            State::Broken(_) => None,
            State::Off(m) | State::Failed(m, _) => Some(m),
            State::On(p) => Some(p.manifest()),
        }
    }
}

impl Permissions {
    /// The permissions as stored when the user allows them: the commands
    /// in full (as hex, to keep the settings file plain), so that any
    /// change to one asks the user again.
    pub fn grant(&self) -> String {
        let commands = self.exec.iter().map(Command::canon).collect::<Vec<_>>().join("\n\n");
        let hex: String = commands.bytes().map(|b| format!("{b:02x}")).collect();
        let files = if self.files.is_some() { ";files=read" } else { "" };
        format!("exec={hex};net={}{files}", self.net.join(","))
    }

    fn programs(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for c in &self.exec {
            if !names.contains(&c.program.as_str()) {
                names.push(&c.program);
            }
        }
        names
    }

    /// What the user is asked to allow, in words.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.exec.is_empty() {
            parts.push(format!("runs {} with your rights", self.programs().join(", ")));
        }
        if !self.net.is_empty() {
            parts.push(format!("connects to {}", self.net.join(", ")));
        }
        if self.files.is_some() {
            parts.push("reads files in the folders you choose".to_string());
        }
        if parts.is_empty() {
            "needs no permissions".into()
        } else {
            parts.join("; ")
        }
    }
}

/// Inside the user's plug-ins folder; hidden, so it is no plug-in.
const JOURNALS: &str = ".journal";

fn journals(roots: &[PathBuf]) -> Option<PathBuf> {
    roots.first().map(|r| r.join(JOURNALS))
}

/// Where the journal of plug-in `id` is.
pub fn journal_path(roots: &[PathBuf], id: &str) -> Option<PathBuf> {
    journals(roots).map(|dir| Journal::path_in(&dir, id))
}

fn journal_of(dir: Option<&Path>, id: &str) -> Journal {
    dir.map_or_else(Journal::default, |d| Journal::new(d, id))
}

/// The folders plug-in `m` may read, as the user wrote them: at first
/// their music folder.
pub fn folders_text(m: &Manifest, config: &PluginConfig, home: Option<&Path>) -> String {
    match config.limit(&m.id, FOLDERS) {
        Some(text) => text.to_string(),
        None => home.map_or_else(String::new, |h| files::shown(&files::music_folder(h), Some(h))),
    }
}

fn listed(m: &Manifest, config: &PluginConfig, key: &str) -> Vec<String> {
    config.limit(&m.id, key).unwrap_or("").split(',').filter(|a| !a.is_empty()).map(str::to_string).collect()
}

/// The addresses of `m` and whether each is open. An ordinary one is
/// until unticked; one that needs a decision is not until ticked.
pub fn addresses(m: &Manifest, config: &PluginConfig) -> Vec<(String, bool)> {
    let (off, on) = (listed(m, config, HOSTS_OFF), listed(m, config, HOSTS_ON));
    m.permissions.net.iter().map(|a| (a.clone(), if needs_decision(a) { on.contains(a) } else { !off.contains(a) })).collect()
}

/// Why address `a` waits for the user's decision.
pub fn caution(a: &str) -> Option<&'static str> {
    NetEntry::parse(a).ok().and_then(|e| e.caution())
}

fn needs_decision(a: &str) -> bool {
    caution(a).is_some()
}

/// What to store under [`HOSTS_OFF`] and [`HOSTS_ON`] for these choices.
pub fn stored_addresses(addresses: &[(String, bool)]) -> (String, String) {
    let joined = |cautious: bool, open: bool| {
        let chosen: Vec<&str> =
            addresses.iter().filter(|(a, o)| needs_decision(a) == cautious && *o == open).map(|(a, _)| a.as_str()).collect();
        chosen.join(",")
    };
    (joined(false, false), joined(true, true))
}

/// The keys of [`PluginConfig::limit`].
pub const FOLDERS: &str = "folders";
pub const HOSTS_OFF: &str = "hosts_off";
pub const HOSTS_ON: &str = "hosts_on";

fn access_of(m: &Manifest, config: &PluginConfig, journals: Option<&Path>, closed: &Closed) -> Access {
    let home = closed.home.as_deref();
    let folders = if m.permissions.files.is_some() { files::parse_folders(&folders_text(m, config, home), home) } else { Vec::new() };
    Access {
        journal: Arc::new(journal_of(journals, &m.id)),
        hosts: addresses(m, config).into_iter().filter(|(_, open)| *open).map(|(a, _)| a).collect(),
        folders,
        closed: closed.clone(),
    }
}

/// The plug-in folders under `roots`, hidden ones skipped, by name. Of
/// two with the same name, the one under the earlier root wins, so the
/// user's copy hides a packaged one.
pub fn folders(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut dirs: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| std::fs::read_dir(root).into_iter().flatten().flatten())
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| seen.insert(p.file_name().map(ToOwned::to_owned)))
        .collect();
    dirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    dirs
}

/// Loads every plug-in under `roots` the user allowed in `config`; none
/// reads what `own`, the player's folders, holds.
pub fn load_all(roots: &[PathBuf], config: &PluginConfig, cx: &Context, own: &[PathBuf]) -> Vec<Installed> {
    let journals = journals(roots);
    let closed = Closed { home: std::env::home_dir(), folders: own.to_vec() };
    folders(roots)
        .into_iter()
        .map(|dir| {
            let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let state = match Manifest::load(&dir) {
                Err(e) => State::Broken(e),
                Ok(m) if config.grant(&m.id) != m.permissions.grant() => State::Off(m),
                Ok(m) => match Plugin::load(&dir, &values(&m, config), cx, access_of(&m, config, journals.as_deref(), &closed)) {
                    Ok(p) => State::On(Arc::new(p)),
                    Err(e) => State::Failed(m, e),
                },
            };
            if let State::Broken(e) | State::Failed(_, e) = &state {
                log::warn!("plug-in {folder}: {e}");
            }
            Installed { folder, dir, state }
        })
        .collect()
}

/// The user's values for the settings of `m` (missing ones are left out).
pub fn values(m: &Manifest, config: &PluginConfig) -> HashMap<String, String> {
    m.settings.iter().filter_map(|s| config.value(&m.id, &s.key).map(|v| (s.key.clone(), v.to_string()))).collect()
}

/// The plug-ins of one kind that are on, in folder order.
fn on(list: &[Installed], wanted: fn(&Kind) -> bool) -> impl Iterator<Item = &Arc<Plugin>> {
    list.iter().filter_map(move |i| match &i.state {
        State::On(p) if wanted(&p.manifest().kind) => Some(p),
        _ => None,
    })
}

/// The visual plug-ins that are on, in folder order.
pub fn visuals(list: &[Installed]) -> Vec<Arc<Plugin>> {
    on(list, |k| *k == Kind::Visual).cloned().collect()
}

/// The sources of the plug-ins that are on, in folder order.
pub fn sources(list: &[Installed]) -> Vec<Arc<dyn Source>> {
    on(list, |k| matches!(k, Kind::Source(_))).map(|p| p.clone() as Arc<dyn Source>).collect()
}

/// An instance of each dsp plug-in that is on, in folder order, for the
/// engine; one that cannot be made is left out.
pub fn effects(list: &[Installed]) -> Vec<Box<dyn Effect>> {
    on(list, |k| *k == Kind::Dsp)
        .filter_map(|p| match p.effect() {
            Ok(e) => Some(Box::new(e) as Box<dyn Effect>),
            Err(e) => {
                log::warn!("plug-in {}: {e}", p.manifest().id);
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, program: &str) -> String {
        let exec = match program {
            "" => String::new(),
            p => format!("[[permissions.exec]]\nid = \"a\"\nprogram = \"{p}\"\nargs = []\n"),
        };
        format!("id = \"{id}\"\nname = \"N\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"source\"\n[source]\nhosts = [\"x.invalid\"]\n{exec}")
    }

    fn grant_of(text: &str) -> String {
        crate::manifest::Manifest::parse(text).unwrap().permissions.grant()
    }

    #[test]
    fn a_plugin_is_on_only_with_the_permissions_it_asks_for_now() {
        let root = std::env::temp_dir().join(format!("popamp-installed-{}", std::process::id()));
        for (id, text) in [("asks", manifest("asks", "echo")), ("broken", "id = 1".into()), ("off", manifest("off", ""))] {
            std::fs::create_dir_all(root.join(id)).unwrap();
            std::fs::write(root.join(id).join("plugin.toml"), text).unwrap();
            std::fs::write(root.join(id).join("plugin.wasm"), b"not wasm").unwrap();
        }
        std::fs::create_dir_all(root.join(".git")).unwrap();
        // "asks" was allowed less than it asks for now; "off" never was.
        let config = |grant: &str| {
            let mut c = popamp_core::config::Config::load(root.join("popamp.ini"));
            c.set_plugin_grant("asks", grant);
            c.plugins()
        };
        let list = load_all(std::slice::from_ref(&root), &config("exec=;net="), &Context::default(), &[]);
        let states: Vec<_> = list
            .iter()
            .map(|i| match &i.state {
                State::Broken(_) => format!("{} broken", i.folder),
                State::Off(_) => format!("{} off", i.folder),
                State::On(_) => format!("{} on", i.folder),
                State::Failed(..) => format!("{} failed", i.folder),
            })
            .collect();
        assert_eq!(states, ["asks off", "broken broken", "off off"]);
        let list = load_all(std::slice::from_ref(&root), &config(&grant_of(&manifest("asks", "echo"))), &Context::default(), &[]);
        assert!(matches!(&list[0].state, State::Failed(_, e) if e.starts_with("cannot compile")), "allowed, so it was loaded");
        assert!(sources(&list).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_users_plugin_folder_hides_a_system_one_of_the_same_name() {
        let root = std::env::temp_dir().join(format!("popamp-installed-dirs-{}", std::process::id()));
        let (user, system) = (root.join("user"), root.join("system"));
        for dir in [user.join("youtube"), system.join("youtube"), system.join("crossfeed"), system.join(".hidden")] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let list = folders(&[user.clone(), system.clone()]);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(list, [system.join("crossfeed"), user.join("youtube")]);
    }

    #[test]
    fn sources_go_before_the_built_in_ones_and_dsp_plugins_to_the_engine() {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let root = std::env::temp_dir().join(format!("popamp-installed-kinds-{}", std::process::id()));
        for id in ["test", "test-dsp"] {
            std::fs::create_dir_all(root.join(id)).unwrap();
            std::fs::copy(fixtures.join(id).join("plugin.toml"), root.join(id).join("plugin.toml")).unwrap();
            std::fs::copy(fixtures.join("test/plugin.wasm"), root.join(id).join("plugin.wasm")).unwrap();
        }
        let mut c = popamp_core::config::Config::load(root.join("popamp.ini"));
        let test = std::fs::read_to_string(root.join("test/plugin.toml")).unwrap();
        c.set_plugin_grant("test", &grant_of(&test));
        c.set_plugin_grant("test-dsp", "exec=;net=");
        let list = load_all(std::slice::from_ref(&root), &c.plugins(), &Context::default(), &[]);
        let ids: Vec<String> = sources(&list).iter().map(|s| s.id().to_string()).collect();
        assert_eq!(ids, ["test"]);
        let mut fx = effects(&list);
        assert_eq!(fx.len(), 1);
        let mut b = vec![1.0; 8192];
        fx[0].reset(8_000);
        fx[0].process(&mut b);
        assert_eq!(b[0], 0.5);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_user_is_told_what_a_plugin_may_do() {
        let command = |id: &str, program: &str| Command { id: id.into(), program: program.into(), args: vec![], vars: Default::default() };
        let p = |exec: &[(&str, &str)], net: &[&str]| Permissions {
            exec: exec.iter().map(|(id, program)| command(id, program)).collect(),
            net: net.iter().map(|s| s.to_string()).collect(),
            files: None,
        };
        let both = p(&[("a", "yt-dlp"), ("b", "yt-dlp"), ("c", "ffmpeg")], &[]);
        assert_eq!(both.describe(), "runs yt-dlp, ffmpeg with your rights");
        assert_eq!(p(&[], &["*.example.com"]).describe(), "connects to *.example.com");
        assert_eq!(p(&[], &[]).describe(), "needs no permissions");
        assert_eq!(p(&[], &["h"]).grant(), "exec=;net=h");
    }

    #[test]
    fn any_change_to_a_command_asks_the_user_again() {
        let grant = grant_of(&manifest("a", "echo"));
        assert_ne!(grant, grant_of(&manifest("a", "printf")));
        assert_eq!(grant, grant_of(&manifest("a", "echo")));
        assert!(!grant.contains("echo"), "plain in the settings file: {grant}");
    }

    #[test]
    fn the_journal_folder_is_not_taken_for_a_plugin() {
        let root = std::env::temp_dir().join(format!("popamp-installed-journal-{}", std::process::id()));
        let path = journal_path(std::slice::from_ref(&root), "yt").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let list = folders(std::slice::from_ref(&root));
        std::fs::remove_dir_all(&root).unwrap();
        assert!(list.is_empty(), "{list:?}");
    }
}
