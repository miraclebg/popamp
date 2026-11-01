//! Native dialogs through `zenity` (or `kdialog`), run off the UI thread.
//! Results come back to the event loop as [`DialogResult`]s.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub enum DialogKind {
    /// Open files; `play` replaces the playlist, otherwise appends.
    OpenFiles {
        play: bool,
    },
    OpenFolder {
        play: bool,
    },
    OpenUrl {
        play: bool,
    },
    LoadPlaylist,
    SavePlaylist,
    SaveHtml,
    JumpToTime,
    JumpToFile,
    EditEntry {
        id: u64,
    },
    EqLoadPreset {
        auto: bool,
    },
    EqSavePreset {
        auto: bool,
    },
    EqDeletePreset {
        auto: bool,
    },
    RemoveBookmark,
    EqImport,
    EqExport,
    Skin,
    InstallSkin,
    Info,
    ConfirmPhysicalRemove {
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct DialogResult {
    pub kind: DialogKind,
    /// Selected paths/strings; empty when cancelled.
    pub values: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Backend {
    Zenity,
    KDialog,
    None,
}

fn backend() -> Backend {
    static B: std::sync::OnceLock<Backend> = std::sync::OnceLock::new();
    *B.get_or_init(|| {
        let has = |bin: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()));
        if has("zenity") {
            Backend::Zenity
        } else if has("kdialog") {
            Backend::KDialog
        } else {
            log::warn!("neither zenity nor kdialog found; dialogs are unavailable");
            Backend::None
        }
    })
}

fn run(cmd: &mut Command) -> Option<String> {
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string();
    Some(s)
}

/// The request for a dialog; built on the UI thread, executed elsewhere.
#[derive(Debug, Clone)]
pub enum Request {
    Files { title: String, multiple: bool, filter: Vec<String>, start: Option<PathBuf> },
    Folder { title: String, start: Option<PathBuf> },
    Save { title: String, name: String, start: Option<PathBuf> },
    Entry { title: String, text: String, initial: String },
    List { title: String, column: String, rows: Vec<String> },
    Info { title: String, text: String },
    Question { title: String, text: String },
}

pub fn execute(req: &Request) -> Vec<String> {
    let b = backend();
    let lines = |s: Option<String>| -> Vec<String> {
        s.map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_string).collect()).unwrap_or_default()
    };
    match (b, req) {
        (Backend::None, _) => vec![],
        (Backend::Zenity, Request::Files { title, multiple, filter, start }) => {
            let mut c = Command::new("zenity");
            c.args(["--file-selection", "--title", title, "--separator", "\n"]);
            if *multiple {
                c.arg("--multiple");
            }
            if !filter.is_empty() {
                c.arg(format!("--file-filter=Media | {}", filter.join(" ")));
                c.arg("--file-filter=All files | *");
            }
            if let Some(s) = start {
                c.arg(format!("--filename={}/", s.display()));
            }
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Folder { title, start }) => {
            let mut c = Command::new("zenity");
            c.args(["--file-selection", "--directory", "--title", title]);
            if let Some(s) = start {
                c.arg(format!("--filename={}/", s.display()));
            }
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Save { title, name, start }) => {
            let mut c = Command::new("zenity");
            c.args(["--file-selection", "--save", "--confirm-overwrite", "--title", title]);
            let initial = start.as_ref().map(|s| s.join(name)).unwrap_or_else(|| PathBuf::from(name));
            c.arg(format!("--filename={}", initial.display()));
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Entry { title, text, initial }) => {
            lines(run(Command::new("zenity").args(["--entry", "--title", title, "--text", text, "--entry-text", initial])))
        }
        (Backend::Zenity, Request::List { title, column, rows }) => {
            let mut c = Command::new("zenity");
            c.args(["--list", "--title", title, "--width", "520", "--height", "480", "--print-column", "1", "--hide-column", "1"]);
            c.args(["--column", "#", "--column", column]);
            for (i, r) in rows.iter().enumerate() {
                c.arg(i.to_string());
                c.arg(r);
            }
            lines(run(&mut c))
        }
        (Backend::Zenity, Request::Info { title, text }) => {
            let _ = run(Command::new("zenity").args(["--info", "--no-markup", "--title", title, "--text", text]));
            vec![]
        }
        (Backend::Zenity, Request::Question { title, text }) => {
            if run(Command::new("zenity").args(["--question", "--no-markup", "--title", title, "--text", text])).is_some() {
                vec!["yes".into()]
            } else {
                vec![]
            }
        }
        (Backend::KDialog, Request::Files { title, multiple, filter, start }) => {
            let mut c = Command::new("kdialog");
            c.args(["--title", title, "--getopenfilename"]);
            c.arg(start.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| ".".into()));
            c.arg(format!("{}|Media files", filter.join(" ")));
            if *multiple {
                c.args(["--multiple", "--separate-output"]);
            }
            lines(run(&mut c))
        }
        (Backend::KDialog, Request::Folder { title, start }) => {
            let start = start.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| ".".into());
            lines(run(Command::new("kdialog").args(["--title", title, "--getexistingdirectory", &start])))
        }
        (Backend::KDialog, Request::Save { title, name, start }) => {
            let initial = start.as_ref().map(|s| s.join(name)).unwrap_or_else(|| PathBuf::from(name));
            lines(run(Command::new("kdialog").args(["--title", title, "--getsavefilename", &initial.display().to_string()])))
        }
        (Backend::KDialog, Request::Entry { title, text, initial }) => {
            lines(run(Command::new("kdialog").args(["--title", title, "--inputbox", text, initial])))
        }
        (Backend::KDialog, Request::List { title, column, rows }) => {
            let mut c = Command::new("kdialog");
            c.args(["--title", title, "--menu", column]);
            for (i, r) in rows.iter().enumerate() {
                c.arg(i.to_string());
                c.arg(r);
            }
            lines(run(&mut c))
        }
        (Backend::KDialog, Request::Info { title, text }) => {
            let _ = run(Command::new("kdialog").args(["--title", title, "--msgbox", text]));
            vec![]
        }
        (Backend::KDialog, Request::Question { title, text }) => {
            if run(Command::new("kdialog").args(["--title", title, "--yesno", text])).is_some() {
                vec!["yes".into()]
            } else {
                vec![]
            }
        }
    }
}

/// Parses `m:ss`, `h:mm:ss` or plain seconds.
pub fn parse_time(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut secs = 0u64;
    for p in &parts {
        secs = secs.checked_mul(60)?.checked_add(p.trim().parse::<u64>().ok()?)?;
    }
    Some(secs * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("1:30"), Some(90_000));
        assert_eq!(parse_time("45"), Some(45_000));
        assert_eq!(parse_time("1:00:01"), Some(3_601_000));
        assert_eq!(parse_time("x"), None);
        assert_eq!(parse_time("1:2:3:4"), None);
    }
}
