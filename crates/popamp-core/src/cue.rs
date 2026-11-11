//! CUE sheets: one audio file (or a few) cut into tracks.
//!
//! A track of a sheet is played from the location `<sheet>#<number>`, so
//! a playlist keeps it like any other file.

use crate::text::decode_lossy;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub number: u32,
    pub file: PathBuf,
    pub title: Option<String>,
    pub performer: Option<String>,
    pub start_ms: u64,
    /// Where the next track of the same file starts; `None` for the last.
    pub end_ms: Option<u64>,
}

impl Sheet {
    pub fn track(&self, number: u32) -> Option<&Track> {
        self.tracks.iter().find(|t| t.number == number)
    }
}

pub fn is_cue(location: &str) -> bool {
    Path::new(location).extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"))
}

pub fn track_location(sheet: &str, number: u32) -> String {
    format!("{sheet}#{number}")
}

/// The sheet and track number of a location made by [`track_location`].
pub fn split_location(location: &str) -> Option<(&str, u32)> {
    let (sheet, number) = location.rsplit_once('#')?;
    if !is_cue(sheet) || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((sheet, number.parse().ok()?))
}

pub fn read(path: &Path) -> std::io::Result<Sheet> {
    let bytes = std::fs::read(path)?;
    Ok(parse(&decode_lossy(&bytes), path.parent().unwrap_or(Path::new(""))))
}

/// The value after a command: quoted, or the first word.
fn value(rest: &str) -> String {
    let rest = rest.trim();
    match rest.strip_prefix('"') {
        Some(q) => q.split('"').next().unwrap_or_default().to_string(),
        None => rest.split_whitespace().next().unwrap_or_default().to_string(),
    }
}

/// `mm:ss:ff`, with 75 frames a second.
fn index_ms(time: &str) -> Option<u64> {
    let mut parts = time.split(':').map(|p| p.trim().parse::<u64>().ok());
    let (m, s, f) = (parts.next()??, parts.next()??, parts.next()??);
    Some((m * 60 + s) * 1000 + f * 1000 / 75)
}

/// The file a sheet names; when it is not there, one of the same name in
/// another format (sheets often outlive a conversion from WAV).
fn find_file(base: &Path, name: &str) -> PathBuf {
    let path = base.join(name.replace('\\', "/"));
    if path.exists() {
        return path;
    }
    crate::COMMON_EXTENSIONS.iter().map(|e| path.with_extension(e)).find(|p| p.exists()).unwrap_or(path)
}

pub fn parse(text: &str, base: &Path) -> Sheet {
    let mut sheet = Sheet::default();
    let mut file: Option<PathBuf> = None;
    // The track being read: its number, and whether it is audio.
    let mut current: Option<Track> = None;
    let mut done: Vec<Track> = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match command.to_ascii_uppercase().as_str() {
            "FILE" => file = Some(find_file(base, &value(rest))),
            "TRACK" => {
                done.extend(current.take().filter(|t| t.start_ms != u64::MAX));
                let mut words = rest.split_whitespace();
                let number = words.next().and_then(|n| n.parse().ok());
                let audio = words.next().is_some_and(|k| k.eq_ignore_ascii_case("AUDIO"));
                if let (Some(number), Some(file), true) = (number, &file, audio) {
                    let file = file.clone();
                    current = Some(Track { number, file, title: None, performer: None, start_ms: u64::MAX, end_ms: None });
                }
            }
            "TITLE" | "PERFORMER" => {
                let v = Some(value(rest)).filter(|v| !v.is_empty());
                let is_title = command.eq_ignore_ascii_case("TITLE");
                match (&mut current, is_title) {
                    (Some(t), true) => t.title = v,
                    (Some(t), false) => t.performer = v,
                    (None, true) => sheet.title = v,
                    (None, false) => sheet.performer = v,
                }
            }
            "INDEX" => {
                let mut words = rest.split_whitespace();
                if let (Some(t), Some("01"), Some(ms)) = (&mut current, words.next(), words.next().and_then(index_ms)) {
                    t.start_ms = ms;
                }
            }
            _ => {}
        }
    }
    done.extend(current.filter(|t| t.start_ms != u64::MAX));
    // A track ends where the next one in its file starts.
    for i in 0..done.len() {
        if let Some(next) = done.get(i + 1).filter(|n| n.file == done[i].file) {
            done[i].end_ms = Some(next.start_ms);
        }
    }
    sheet.tracks = done;
    sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: &str = "\u{feff}REM GENRE Rock
PERFORMER \"The Band\"
TITLE \"Live\"
FILE \"live.wav\" WAVE
  TRACK 01 AUDIO
    TITLE \"Intro\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Song\"
    PERFORMER \"Guest\"
    INDEX 00 03:58:00
    INDEX 01 04:00:37
  TRACK 03 MODE1/2352
    INDEX 01 08:00:00
FILE \"bonus.flac\" WAVE
  TRACK 04 AUDIO
    TITLE Bonus
    INDEX 01 00:00:00
";

    #[test]
    fn a_sheet_cuts_its_files_into_tracks() {
        let sheet = parse(SHEET, Path::new("/music"));
        assert_eq!((sheet.title.as_deref(), sheet.performer.as_deref()), (Some("Live"), Some("The Band")));
        let t = &sheet.tracks;
        assert_eq!(t.iter().map(|t| t.number).collect::<Vec<_>>(), [1, 2, 4], "the data track is left out");
        assert_eq!((t[0].title.as_deref(), t[0].start_ms, t[0].end_ms), (Some("Intro"), 0, Some(240_493)));
        assert_eq!((t[1].performer.as_deref(), t[1].start_ms, t[1].end_ms), (Some("Guest"), 240_493, None));
        assert_eq!(t[1].file, Path::new("/music/live.wav"));
        assert_eq!((t[2].title.as_deref(), t[2].file.as_path(), t[2].end_ms), (Some("Bonus"), Path::new("/music/bonus.flac"), None));
    }

    #[test]
    fn a_named_file_that_was_converted_is_found_by_its_name() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("live.flac"), b"").unwrap();
        assert_eq!(parse(SHEET, &dir).tracks[0].file, dir.join("live.flac"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_track_location_names_the_sheet_and_the_number() {
        let loc = track_location("/music/Live.CUE", 2);
        assert_eq!(split_location(&loc), Some(("/music/Live.CUE", 2)));
        assert_eq!(split_location("/music/a.flac#2"), None);
        assert_eq!(split_location("/music/a.cue"), None);
        assert_eq!(split_location("/music/a.cue#x"), None);
        assert!(is_cue("/music/a.cue") && !is_cue("/music/a.flac"));
    }
}
