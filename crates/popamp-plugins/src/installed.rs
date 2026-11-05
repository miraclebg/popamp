//! The plug-ins in the plug-ins folder and what became of each: broken,
//! off, on, or failed to load. A plug-in is on only while the permissions
//! the user allowed are the ones its manifest asks for now, so a new
//! version that asks for more waits to be allowed again.

use crate::host::Plugin;
use crate::manifest::{Manifest, Permissions};
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
    /// The permissions as stored when the user allows them.
    pub fn grant(&self) -> String {
        format!("exec={};net={}", self.exec.join(","), self.net.join(","))
    }

    /// What the user is asked to allow, in words.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.exec.is_empty() {
            parts.push(format!("runs {} with your rights", self.exec.join(", ")));
        }
        if !self.net.is_empty() {
            parts.push(format!("connects to {}", self.net.join(", ")));
        }
        if parts.is_empty() {
            "needs no permissions".into()
        } else {
            parts.join("; ")
        }
    }
}

/// The plug-in folders under `root`, hidden ones skipped, by name.
pub fn folders(root: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> =
        rd.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs
}

/// Loads every plug-in under `root` the user allowed in `config`.
pub fn load_all(root: &Path, config: &PluginConfig, cx: &Context) -> Vec<Installed> {
    folders(root)
        .into_iter()
        .map(|dir| {
            let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let state = match Manifest::load(&dir) {
                Err(e) => State::Broken(e),
                Ok(m) if config.grant(&m.id) != m.permissions.grant() => State::Off(m),
                Ok(m) => match Plugin::load(&dir, &values(&m, config), cx) {
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

/// The sources of the plug-ins that are on, in folder order.
pub fn sources(list: &[Installed]) -> Vec<Arc<dyn Source>> {
    list.iter()
        .filter_map(|i| match &i.state {
            State::On(p) => Some(p.clone() as Arc<dyn Source>),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, exec: &str) -> String {
        format!("id = \"{id}\"\nname = \"N\"\nversion = \"1\"\napi = \"1.0\"\nkind = \"source\"\n[source]\nhosts = [\"x.invalid\"]\n[permissions]\nexec = [{exec}]\n")
    }

    #[test]
    fn a_plugin_is_on_only_with_the_permissions_it_asks_for_now() {
        let root = std::env::temp_dir().join(format!("popamp-installed-{}", std::process::id()));
        for (id, text) in [("asks", manifest("asks", "\"sh\"")), ("broken", "id = 1".into()), ("off", manifest("off", ""))] {
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
        let list = load_all(&root, &config("exec=;net="), &Context::default());
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
        let list = load_all(&root, &config("exec=sh;net="), &Context::default());
        assert!(matches!(&list[0].state, State::Failed(_, e) if e.starts_with("cannot compile")), "allowed, so it was loaded");
        assert!(sources(&list).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_user_is_told_what_a_plugin_may_do() {
        let p = |exec: &[&str], net: &[&str]| Permissions {
            exec: exec.iter().map(|s| s.to_string()).collect(),
            net: net.iter().map(|s| s.to_string()).collect(),
        };
        assert_eq!(p(&["yt-dlp", "ffmpeg"], &[]).describe(), "runs yt-dlp, ffmpeg with your rights");
        assert_eq!(p(&[], &["*.example.com"]).describe(), "connects to *.example.com");
        assert_eq!(p(&[], &[]).describe(), "needs no permissions");
        assert_eq!(p(&["a", "b"], &["h"]).grant(), "exec=a,b;net=h");
    }
}
