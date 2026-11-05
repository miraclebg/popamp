//! `plugin.toml`: what a plug-in is, which locations it takes and what it
//! may do. Checked strictly, like `skin.toml`: an unknown key, a bad value
//! or a missing module keeps the plug-in off, with the error shown.

use popamp_plugin_api::{Version, VERSION};
use serde::Deserialize;
use std::path::Path;

pub const FILE: &str = "plugin.toml";
pub const MODULE: &str = "plugin.wasm";

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: Version,
    pub description: String,
    pub kind: Kind,
    pub permissions: Permissions,
    pub settings: Vec<Setting>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Source(SourceSpec),
}

/// Which locations a source takes, so the player can tell without
/// calling the plug-in.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    /// `example.com` takes that host only; `*.example.com` its subdomains.
    pub hosts: Vec<String>,
    /// True when opening takes long enough to say "connecting".
    #[serde(default)]
    pub slow: bool,
}

impl SourceSpec {
    /// True when `location` is an http(s) URL on one of the hosts.
    pub fn takes(&self, location: &str) -> bool {
        let Some(host) = host_of(location) else { return false };
        self.hosts.iter().any(|p| match p.strip_prefix("*.") {
            Some(domain) => host.strip_suffix(domain).is_some_and(|sub| sub.ends_with('.') && sub.len() > 1),
            None => host == *p,
        })
    }
}

/// The host of an http(s) URL, lowercase, without user or port.
fn host_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.to_ascii_lowercase()).filter(|h| !h.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    /// Programs it may start, by name; the setting of the same key, if
    /// there is one, says where each program is.
    pub exec: Vec<String>,
    /// Hosts it may reach over HTTP(S).
    pub net: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: SettingKind,
    pub default: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingKind {
    /// A program or file: a path, or a name looked up in `PATH`.
    Path,
    Text,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    id: String,
    name: String,
    version: String,
    api: String,
    kind: String,
    #[serde(default)]
    description: String,
    source: Option<SourceSpec>,
    #[serde(default)]
    permissions: Permissions,
    #[serde(default, rename = "setting")]
    settings: Vec<Setting>,
}

impl Manifest {
    /// Reads `<dir>/plugin.toml` and checks it against the folder.
    pub fn load(dir: &Path) -> Result<Manifest, String> {
        let text = std::fs::read_to_string(dir.join(FILE)).map_err(|e| format!("{FILE}: {e}"))?;
        let m = Manifest::parse(&text)?;
        let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if folder != m.id {
            return Err(format!("id {:?} differs from its folder {folder:?}", m.id));
        }
        if !dir.join(MODULE).is_file() {
            return Err(format!("{MODULE} is missing"));
        }
        Ok(m)
    }

    pub fn parse(text: &str) -> Result<Manifest, String> {
        let f: File = toml::from_str(text).map_err(|e| format!("{FILE}: {}", e.to_string().trim_end()))?;
        check_id(&f.id)?;
        let api = check_api(&f.api)?;
        let kind = kind_of(&f.kind, f.source)?;
        check_exec(&f.permissions)?;
        check_settings(&f.settings)?;
        Ok(Manifest {
            id: f.id,
            name: f.name,
            version: f.version,
            api,
            description: f.description,
            kind,
            permissions: f.permissions,
            settings: f.settings,
        })
    }
}

fn check_id(id: &str) -> Result<(), String> {
    match !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
        true => Ok(()),
        false => Err(format!("id {id:?} must be [a-z0-9-]")),
    }
}

fn check_api(api: &str) -> Result<Version, String> {
    let api: Version = api.parse()?;
    match VERSION.loads(api) {
        true => Ok(api),
        false => Err(format!("built for plug-in API {api}, this Popamp has {VERSION}")),
    }
}

fn kind_of(kind: &str, source: Option<SourceSpec>) -> Result<Kind, String> {
    match (kind, source) {
        ("source", Some(spec)) => Ok(Kind::Source(check_hosts(spec)?)),
        ("source", None) => Err("a source needs a [source] table with its hosts".into()),
        (other, _) => Err(format!("unknown kind {other:?}; this Popamp knows \"source\"")),
    }
}

fn check_exec(p: &Permissions) -> Result<(), String> {
    match p.exec.iter().find(|p| p.is_empty() || p.contains('/')) {
        Some(p) => Err(format!("exec {p:?} must be a program name, not a path")),
        None => Ok(()),
    }
}

fn check_settings(settings: &[Setting]) -> Result<(), String> {
    match settings.iter().enumerate().find(|(i, s)| settings[..*i].iter().any(|o| o.key == s.key)) {
        Some((_, s)) => Err(format!("setting {:?} is given twice", s.key)),
        None => Ok(()),
    }
}

fn check_hosts(spec: SourceSpec) -> Result<SourceSpec, String> {
    if spec.hosts.is_empty() {
        return Err("[source] hosts is empty".into());
    }
    let valid = |h: &str| {
        let name = h.strip_prefix("*.").unwrap_or(h);
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    };
    match spec.hosts.iter().find(|h| !valid(h)) {
        Some(h) => Err(format!("host {h:?} must be a lowercase name, or *. and a name")),
        None => Ok(spec),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YOUTUBE: &str = r#"
id = "youtube"
name = "YouTube"
version = "1.0.0"
api = "1.0"
kind = "source"
description = "Plays YouTube videos."

[source]
hosts = ["youtube.com", "*.youtube.com", "youtu.be"]
slow = true

[permissions]
exec = ["yt-dlp", "ffmpeg"]

[[setting]]
key = "yt-dlp"
label = "yt-dlp program"
type = "path"
default = "yt-dlp"
"#;

    fn with(change: (&str, &str)) -> Result<Manifest, String> {
        assert!(YOUTUBE.contains(change.0), "{}", change.0);
        Manifest::parse(&YOUTUBE.replacen(change.0, change.1, 1))
    }

    #[test]
    fn reads_a_source_manifest() {
        let m = Manifest::parse(YOUTUBE).unwrap();
        assert_eq!((m.id.as_str(), m.api.to_string(), m.version.as_str()), ("youtube", "1.0".into(), "1.0.0"));
        let Kind::Source(spec) = &m.kind;
        assert!(spec.slow);
        assert_eq!(m.permissions.exec, ["yt-dlp", "ffmpeg"]);
        assert!(m.permissions.net.is_empty());
        assert_eq!(m.settings[0].kind, SettingKind::Path);
    }

    #[test]
    fn a_source_takes_urls_on_its_hosts_only() {
        let m = Manifest::parse(YOUTUBE).unwrap();
        let Kind::Source(spec) = &m.kind;
        for yes in
            ["https://youtube.com/watch?v=a", "http://www.youtube.com/x", "https://user@music.youtube.com:443/", "https://YOUTU.BE/a"]
        {
            assert!(spec.takes(yes), "{yes}");
        }
        for no in [
            "https://notyoutube.com/",
            "https://youtube.com.evil.org/",
            "ftp://youtube.com/a",
            "/music/youtube.com",
            "https://.youtube.com/",
        ] {
            assert!(!spec.takes(no), "{no}");
        }
    }

    #[test]
    fn a_bad_manifest_says_what_is_wrong() {
        let too_new = format!("built for plug-in API 2.0, this Popamp has {VERSION}");
        for (change, error) in [
            (("api = \"1.0\"", "api = \"2.0\""), too_new.as_str()),
            (("api = \"1.0\"", "api = \"1\""), "bad API version \"1\", expected MAJOR.MINOR"),
            (("id = \"youtube\"", "id = \"You Tube\""), "id \"You Tube\" must be [a-z0-9-]"),
            (("kind = \"source\"", "kind = \"vis\""), "unknown kind \"vis\"; this Popamp knows \"source\""),
            (("[source]", "[other]"), "unknown field"),
            (("exec = [\"yt-dlp\"", "exec = [\"/usr/bin/yt-dlp\""), "exec \"/usr/bin/yt-dlp\" must be a program name, not a path"),
            (("\"*.youtube.com\"", "\"*.YouTube.com\""), "host \"*.YouTube.com\" must be a lowercase name, or *. and a name"),
            (("type = \"path\"", "type = \"colour\""), "unknown variant"),
            (("slow = true", "slow = true\ncolour = 1"), "unknown field `colour`"),
        ] {
            let err = with(change).unwrap_err();
            assert!(err.contains(error), "{change:?}: {err}");
        }
        let twice = format!("{YOUTUBE}\n[[setting]]\nkey = \"yt-dlp\"\nlabel = \"again\"\ntype = \"text\"\ndefault = \"\"\n");
        assert_eq!(Manifest::parse(&twice).unwrap_err(), "setting \"yt-dlp\" is given twice");
        let no_source = YOUTUBE.split("[source]").next().unwrap().to_string() + "[permissions]\n";
        assert_eq!(Manifest::parse(&no_source).unwrap_err(), "a source needs a [source] table with its hosts");
    }

    #[test]
    fn a_plugin_folder_must_match_its_id_and_hold_the_module() {
        let root = std::env::temp_dir().join(format!("popamp-manifest-{}", std::process::id()));
        let dir = root.join("youtube");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), YOUTUBE).unwrap();
        assert_eq!(Manifest::load(&dir).unwrap_err(), "plugin.wasm is missing");
        std::fs::write(dir.join(MODULE), b"").unwrap();
        assert!(Manifest::load(&dir).is_ok());
        let other = root.join("yt");
        std::fs::rename(&dir, &other).unwrap();
        assert_eq!(Manifest::load(&other).unwrap_err(), "id \"youtube\" differs from its folder \"yt\"");
        std::fs::remove_dir_all(root).unwrap();
    }
}
