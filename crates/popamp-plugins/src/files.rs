//! `popamp_list` and `popamp_read`: a plug-in reads only inside the folders
//! the user chose, and never what holds keys, passwords or settings, whatever
//! was chosen. Paths are compared after links are followed, so a link inside
//! a chosen folder leads nowhere else.

use crate::journal::Journal;
use popamp_plugin_api::{Entry, FileData, Listing, ReadRequest};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const MAX_READ: u32 = 1 << 20;
const MAX_ENTRIES: usize = 20_000;
/// Where Windows and macOS keep what `.config` and `.ssh` hold elsewhere.
const CLOSED_IN_HOME: [&str; 2] = ["appdata", "library"];

/// What stays closed whatever folder the user chooses.
#[derive(Debug, Clone, Default)]
pub struct Closed {
    pub home: Option<PathBuf>,
    /// The player's own folders.
    pub folders: Vec<PathBuf>,
}

impl Closed {
    /// Hidden files and folders (`.ssh`, `.gnupg`, `.config`, `.mozilla`…),
    /// the same things under another system's name, and the player's own.
    /// `path` has its links followed.
    fn holds(&self, path: &Path) -> bool {
        if self.folders.iter().any(|f| path.starts_with(f)) {
            return true;
        }
        let in_home = self.home.as_ref().and_then(|h| path.strip_prefix(h).ok());
        let first_is_system = in_home
            .and_then(|p| p.components().next())
            .is_some_and(|c| CLOSED_IN_HOME.iter().any(|name| c.as_os_str().eq_ignore_ascii_case(name)));
        first_is_system || in_home.unwrap_or(path).components().any(is_hidden)
    }

    fn with_links_followed(self) -> Closed {
        let real = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
        Closed { home: self.home.map(real), folders: self.folders.into_iter().map(real).collect() }
    }
}

fn is_hidden(c: Component) -> bool {
    matches!(c, Component::Normal(name) if name.as_encoded_bytes().starts_with(b"."))
}

#[derive(Debug, Default)]
pub struct Folders {
    roots: Vec<PathBuf>,
    closed: Closed,
    journal: Arc<Journal>,
}

impl Folders {
    /// `chosen` are the user's; one that is missing or closed gives nothing.
    pub fn new(chosen: &[PathBuf], closed: Closed, journal: Arc<Journal>) -> Folders {
        let closed = closed.with_links_followed();
        let roots = chosen.iter().filter_map(|f| std::fs::canonicalize(f).ok()).filter(|f| f.is_dir() && !closed.holds(f)).collect();
        Folders { roots, closed, journal }
    }

    /// The real place of `path`, if the plug-in may open it.
    fn open(&self, path: &str) -> Result<PathBuf, String> {
        let real = std::fs::canonicalize(path).map_err(|_| "no such file or folder here".to_string())?;
        match self.roots.iter().any(|r| real.starts_with(r)) && !self.closed.holds(&real) {
            true => Ok(real),
            false => Err("not in the folders you allowed".into()),
        }
    }

    fn noted<T>(&self, action: &str, asked: &str, done: Result<T, String>) -> Result<T, String> {
        match &done {
            Ok(_) => self.journal.record(action, asked, "allowed"),
            Err(why) => self.journal.record(action, &format!("{asked} ({why})"), "refused"),
        }
        done
    }

    /// What a folder holds; an empty `path` gives the chosen folders.
    pub fn list(&self, path: &str) -> Listing {
        match self.noted("list", path, self.entries(path)) {
            Ok(entries) => Listing { error: None, entries },
            Err(error) => Listing { error: Some(error), entries: Vec::new() },
        }
    }

    fn entries(&self, path: &str) -> Result<Vec<Entry>, String> {
        if path.is_empty() {
            return Ok(self.roots.iter().filter_map(|r| entry_of(r)).collect());
        }
        let dir = std::fs::read_dir(self.open(path)?).map_err(|e| e.to_string())?;
        let shown = dir.flatten().map(|e| e.path()).filter_map(|p| self.open(p.to_str()?).ok());
        Ok(shown.filter_map(|p| entry_of(&p)).take(MAX_ENTRIES).collect())
    }

    /// A piece of a file; the journal gets where and how much, not the bytes.
    pub fn read(&self, req: &ReadRequest) -> FileData {
        let asked = format!("{} from {}, {} bytes", req.path, req.offset, req.length);
        match self.noted("read", &asked, self.piece(req)) {
            Ok(data) => FileData { error: None, data },
            Err(error) => FileData { error: Some(error), data: Vec::new() },
        }
    }

    fn piece(&self, req: &ReadRequest) -> Result<Vec<u8>, String> {
        let real = self.open(&req.path)?;
        if !std::fs::metadata(&real).is_ok_and(|m| m.is_file()) {
            return Err("not a file".into());
        }
        let mut file = File::open(real).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(req.offset)).map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        file.take(u64::from(req.length.min(MAX_READ))).read_to_end(&mut data).map_err(|e| e.to_string())?;
        Ok(data)
    }
}

fn entry_of(path: &Path) -> Option<Entry> {
    let meta = std::fs::metadata(path).ok()?;
    let size = if meta.is_dir() { 0 } else { meta.len() };
    Some(Entry { path: path.to_str()?.to_string(), folder: meta.is_dir(), size })
}

/// The folders written in a setting: separated by `;`, `~` for the home.
pub fn parse_folders(text: &str, home: Option<&Path>) -> Vec<PathBuf> {
    text.split(';').map(str::trim).filter(|t| !t.is_empty()).filter_map(|t| expanded(t, home)).filter(|p| p.is_absolute()).collect()
}

fn expanded(text: &str, home: Option<&Path>) -> Option<PathBuf> {
    match text.strip_prefix('~') {
        None => Some(PathBuf::from(text)),
        Some("") => home.map(Path::to_path_buf),
        Some(rest) => rest.strip_prefix(['/', '\\']).and_then(|r| Some(home?.join(r))),
    }
}

/// The user's music folder: named by their desktop, which translates it.
pub fn music_folder(home: &Path) -> PathBuf {
    let dirs = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| home.join(".config"), PathBuf::from).join("user-dirs.dirs");
    std::fs::read_to_string(dirs).ok().and_then(|text| music_in_user_dirs(&text, home)).unwrap_or_else(|| home.join("Music"))
}

fn music_in_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    let line = text.lines().find_map(|l| l.trim().strip_prefix("XDG_MUSIC_DIR="))?;
    let value = line.trim().trim_matches('"');
    match value.strip_prefix("$HOME") {
        Some(rest) => Some(home.join(rest.trim_start_matches('/'))),
        None => Some(PathBuf::from(value)).filter(|p| p.is_absolute()),
    }
}

/// `~` for the home, so a setting stays short and can be read.
pub fn shown(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str() == OsStr::new("") => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home with music, keys, a browser, settings and a link out.
    struct Home {
        dir: PathBuf,
    }

    impl Home {
        fn new(name: &str) -> Home {
            let dir = std::fs::canonicalize(std::env::temp_dir()).unwrap().join(format!("popamp-files-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            for (file, text) in [
                ("Music/a.txt", "abcdefgh"),
                ("Music/sub/b.txt", "b"),
                ("Music/.hidden", "h"),
                ("Other/c.txt", "c"),
                (".ssh/id_rsa", "secret"),
                (".mozilla/profile/logins.json", "secret"),
                ("AppData/Roaming/x", "secret"),
                ("Library/Keychains/x", "secret"),
                ("portable/config.ini", "secret"),
            ] {
                std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
                std::fs::write(dir.join(file), text).unwrap();
            }
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(dir.join(".ssh/id_rsa"), dir.join("Music/key")).unwrap();
                std::os::unix::fs::symlink(dir.join("Other"), dir.join("Music/other")).unwrap();
            }
            Home { dir }
        }

        fn path(&self, p: &str) -> String {
            self.dir.join(p).to_str().unwrap().to_string()
        }

        fn folders(&self, chosen: &[&str]) -> Folders {
            let chosen: Vec<PathBuf> = chosen.iter().map(|c| self.dir.join(c)).collect();
            let closed = Closed { home: Some(self.dir.clone()), folders: vec![self.dir.join("portable")] };
            Folders::new(&chosen, closed, Arc::default())
        }

        fn read(&self, folders: &Folders, p: &str) -> FileData {
            folders.read(&ReadRequest { path: self.path(p), offset: 0, length: 100 })
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_plugin_reads_inside_the_chosen_folder_only() {
        let home = Home::new("inside");
        let f = home.folders(&["Music"]);
        assert_eq!(home.read(&f, "Music/a.txt").data, b"abcdefgh");
        assert_eq!(home.read(&f, "Music/sub/b.txt").data, b"b");
        assert_eq!(home.read(&f, "Other/c.txt").error.as_deref(), Some("not in the folders you allowed"));
        assert!(home.read(&f, "Music/../Other/c.txt").error.is_some(), "going up does not get out");
    }

    #[test]
    fn a_piece_is_read_from_where_it_is_asked_and_no_longer_than_allowed() {
        let home = Home::new("piece");
        let f = home.folders(&["Music"]);
        let data = f.read(&ReadRequest { path: home.path("Music/a.txt"), offset: 2, length: 3 }).data;
        assert_eq!(data, b"cde");
        let big = home.dir.join("Music/big");
        std::fs::write(&big, vec![0u8; MAX_READ as usize + 5]).unwrap();
        let data = f.read(&ReadRequest { path: big.to_str().unwrap().into(), offset: 0, length: u32::MAX }).data;
        assert_eq!(data.len(), MAX_READ as usize);
        assert_eq!(f.read(&ReadRequest { path: home.path("Music/sub"), offset: 0, length: 1 }).error.as_deref(), Some("not a file"));
    }

    #[test]
    #[cfg(unix)]
    fn a_link_leads_nowhere_outside() {
        let home = Home::new("links");
        let f = home.folders(&["Music"]);
        assert!(home.read(&f, "Music/key").error.is_some());
        assert!(home.read(&f, "Music/other/c.txt").error.is_some());
        let names: Vec<String> = f.list(&home.path("Music")).entries.into_iter().map(|e| e.path).collect();
        assert!(names.iter().all(|n| !n.ends_with("/key") && !n.ends_with("/other")), "{names:?}");
    }

    #[test]
    fn the_whole_home_still_keeps_keys_browsers_and_settings_closed() {
        let home = Home::new("whole");
        let f = home.folders(&[""]);
        assert_eq!(home.read(&f, "Other/c.txt").data, b"c");
        for closed in [
            ".ssh/id_rsa",
            ".mozilla/profile/logins.json",
            "AppData/Roaming/x",
            "Library/Keychains/x",
            "portable/config.ini",
            "Music/.hidden",
        ] {
            assert!(home.read(&f, closed).error.is_some(), "{closed}");
        }
        let names: Vec<String> = f.list(&home.path("")).entries.into_iter().map(|e| e.path).collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn choosing_a_closed_folder_opens_nothing() {
        let home = Home::new("closed");
        let f = home.folders(&[".ssh", "portable", "AppData", "missing"]);
        assert!(f.list("").entries.is_empty());
        assert!(home.read(&f, ".ssh/id_rsa").error.is_some());
    }

    #[test]
    fn an_empty_path_lists_the_chosen_folders_and_nothing_is_open_without_any() {
        let home = Home::new("roots");
        let f = home.folders(&["Music", "Other"]);
        let listed: Vec<(String, bool)> = f.list("").entries.into_iter().map(|e| (e.path, e.folder)).collect();
        assert_eq!(listed, [(home.path("Music"), true), (home.path("Other"), true)]);
        let none = home.folders(&[]);
        assert!(none.list("").entries.is_empty());
        assert!(home.read(&none, "Music/a.txt").error.is_some());
    }

    #[test]
    fn each_request_is_in_the_journal_without_the_contents() {
        let home = Home::new("journal");
        let log = home.dir.join("log");
        let closed = Closed { home: Some(home.dir.clone()), folders: Vec::new() };
        let f = Folders::new(&[home.dir.join("Music")], closed, Arc::new(Journal::new(&log, "p")));
        home.read(&f, "Music/a.txt");
        home.read(&f, ".ssh/id_rsa");
        let text = std::fs::read_to_string(Journal::path_in(&log, "p")).unwrap();
        assert!(text.contains(&format!(" read allowed: {} from 0, 100 bytes\n", home.path("Music/a.txt"))), "{text}");
        assert!(text.contains(" read refused: ") && text.contains("id_rsa"), "{text}");
        assert!(!text.contains("abcdefgh") && !text.contains("secret"), "{text}");
    }

    /// Where an absolute path starts: on Windows it needs a drive.
    const ROOT: &str = if cfg!(windows) { "C:" } else { "" };

    fn absolute(path: &str) -> PathBuf {
        PathBuf::from(format!("{ROOT}{path}"))
    }

    #[test]
    fn folders_are_written_with_semicolons_and_a_tilde() {
        let home = absolute("/home/u");
        let home = Some(home.as_path());
        let got = parse_folders(&format!(" ~/Music ; {ROOT}/mnt/a b;; relative ; ~ ;~other"), home);
        assert_eq!(got, [absolute("/home/u/Music"), absolute("/mnt/a b"), absolute("/home/u")]);
        assert_eq!(parse_folders("~/x", None), Vec::<PathBuf>::new());
        assert_eq!(shown(&absolute("/home/u/Music"), home), "~/Music");
        assert_eq!(shown(&absolute("/home/u"), home), "~");
        assert_eq!(shown(&absolute("/mnt/a"), home), format!("{ROOT}/mnt/a"));
    }

    #[test]
    fn the_music_folder_has_the_name_the_desktop_gave_it() {
        let home = absolute("/home/u");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/x\"\nXDG_MUSIC_DIR=\"$HOME/Музыка\"\n";
        assert_eq!(music_in_user_dirs(text, &home), Some(absolute("/home/u/Музыка")));
        assert_eq!(music_in_user_dirs(&format!("XDG_MUSIC_DIR=\"{ROOT}/data/m\""), &home), Some(absolute("/data/m")));
        assert_eq!(music_in_user_dirs("", &home), None);
    }
}
