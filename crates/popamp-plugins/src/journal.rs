//! What a plug-in asked the player to do, allowed or not, in a text file
//! of its own that the user can read. Values, never file contents.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A journal longer than this moves to `<id>.log.old`, replacing the last.
const MAX_BYTES: u64 = 256 * 1024;

#[derive(Debug, Default)]
pub struct Journal {
    /// `None` keeps nothing, for a plug-in that cannot ask for anything.
    path: Option<PathBuf>,
    writing: Mutex<()>,
}

impl Journal {
    pub fn path_in(dir: &Path, id: &str) -> PathBuf {
        dir.join(format!("{id}.log"))
    }

    pub fn new(dir: &Path, id: &str) -> Journal {
        Journal { path: Some(Journal::path_in(dir, id)), writing: Mutex::new(()) }
    }

    /// Adds a line; failing to is no reason to stop the plug-in.
    pub fn record(&self, action: &str, what: &str, outcome: &str) {
        let Some(path) = &self.path else { return };
        let _writing = self.writing.lock().unwrap_or_else(|e| e.into_inner());
        let line = format!("{} {action} {outcome}: {}\n", stamp(now_secs()), one_line(what));
        if let Err(e) = append(path, &line) {
            log::warn!("{}: {e}", path.display());
        }
    }
}

fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let mut old = path.as_os_str().to_owned();
        old.push(".old");
        std::fs::rename(path, old)?;
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}

/// A plug-in's value cannot start a line that looks like the player's.
fn one_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `2026-11-08 13:27:26Z`.
fn stamp(secs: u64) -> String {
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Days to a civil date, in eras of 400 years from 0000-03-01.
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z", rest / 3600, rest / 60 % 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("popamp-journal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_time_reads_as_a_date() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(stamp(1_794_144_449), "2026-11-08 13:27:29Z");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00Z");
    }

    #[test]
    fn each_request_is_one_line_a_plugin_cannot_forge() {
        let dir = folder("lines");
        let journal = Journal::new(&dir, "yt");
        journal.record("exec", "play url=a", "allowed");
        journal.record("exec", "x\n2026-01-01 00:00:00Z exec allowed: fake", "refused");
        let text = std::fs::read_to_string(Journal::path_in(&dir, "yt")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].ends_with(" exec allowed: play url=a"), "{}", lines[0]);
        assert!(lines[1].ends_with(" exec refused: x 2026-01-01 00:00:00Z exec allowed: fake"), "{}", lines[1]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_long_journal_moves_aside_and_keeps_only_the_last_one() {
        let dir = folder("rotate");
        let journal = Journal::new(&dir, "yt");
        let long = "x".repeat(MAX_BYTES as usize + 1);
        journal.record("exec", &long, "allowed");
        journal.record("exec", "second", "allowed");
        journal.record("exec", "third", "allowed");
        let new = std::fs::read_to_string(Journal::path_in(&dir, "yt")).unwrap();
        assert!(new.contains("second") && new.contains("third") && !new.contains("xxx"), "{new}");
        assert!(std::fs::read_to_string(dir.join("yt.log.old")).unwrap().contains("xxx"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn without_a_folder_nothing_is_kept_and_nothing_fails() {
        Journal::default().record("exec", "x", "allowed");
    }
}
