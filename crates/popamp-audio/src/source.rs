//! Sources: who takes a location, lists it, describes it and says how to
//! play it. Files and HTTP are built in; plug-ins (with `--features plugins`) are
//! sources too, asked first. A source never handles audio: it answers
//! with a [`Playback`] and the engine plays.
//!
//! Every call but `handles` may be slow (a network or a program) and runs
//! on a worker thread, never on the engine or UI thread.

use crate::pipe::Pipe;
use parking_lot::{Condvar, Mutex};
use popamp_core::cue;
use popamp_core::meta::TrackMeta;
use popamp_core::playlist::is_url;
use popamp_core::plformats::{self, is_playlist, Item};
use popamp_core::{is_audio_file, text};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// What a call needs from the player's settings.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// HTTP proxy (`[user:pass@]host:port`), empty for none.
    pub proxy: String,
    /// The SoundFont that plays MIDI, empty for the system's.
    pub soundfont: String,
}

/// How the engine plays a track.
#[derive(Debug, Clone, PartialEq)]
pub enum Playback {
    File(PathBuf),
    /// From `start_ms` to `end_ms` (or the end) of a local file, with
    /// the tags of that part rather than the file's.
    Part {
        path: PathBuf,
        start_ms: u64,
        end_ms: Option<u64>,
        meta: Box<TrackMeta>,
    },
    Http(String),
    Pipe(Box<Pipe>),
}

pub trait Source: Send + Sync {
    /// A short name, `[a-z0-9-]`, as a plug-in's id.
    fn id(&self) -> &str;
    /// True if this source takes `location`; quick: no network, no program.
    fn handles(&self, location: &str) -> bool;
    /// The tracks of a list (a folder, a playlist, a channel), or `None`
    /// when `location` is one track.
    fn expand(&self, location: &str, cx: &Context) -> Result<Option<Vec<Item>>, String>;
    /// Tags of a track, or `None` when this source has none to give.
    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>>;
    fn open(&self, location: &str, cx: &Context) -> Result<Playback, String>;
    /// True when opening takes long enough to say "connecting".
    fn slow(&self) -> bool {
        false
    }
}

/// The sources in the order they are asked: plug-ins, then HTTP and
/// files. Shared by every thread that asks; plug-ins can be swapped while
/// it runs.
pub struct Sources {
    /// `None` until the plug-ins are known: they load on a thread of their
    /// own while the player starts.
    plugins: Mutex<Option<Vec<Arc<dyn Source>>>>,
    loaded: Condvar,
    built_in: Vec<Arc<dyn Source>>,
}

/// How long a worker waits for the plug-ins before it does without them.
const PLUGINS_WAIT: Duration = Duration::from_secs(30);

impl Sources {
    /// Sources whose plug-ins are still loading: [`Sources::of`] waits
    /// for them, so a track given at startup goes to its plug-in.
    pub fn waiting() -> Sources {
        Sources { plugins: Mutex::new(None), ..Sources::default() }
    }

    /// Replaces the plug-ins asked before the built-in sources.
    pub fn set_plugins(&self, plugins: Vec<Arc<dyn Source>>) {
        *self.plugins.lock() = Some(plugins);
        self.loaded.notify_all();
    }

    /// The source that takes `location`; anything not a URL is a file.
    /// Waits while the plug-ins load, so call it on a worker thread.
    pub fn of(&self, location: &str) -> Arc<dyn Source> {
        let mut plugins = self.plugins.lock();
        if plugins.is_none() && self.loaded.wait_while_for(&mut plugins, |p| p.is_none(), PLUGINS_WAIT).timed_out() {
            log::warn!("plug-ins did not load in {PLUGINS_WAIT:?}; going on without them");
            *plugins = Some(Vec::new());
        }
        self.pick(plugins.as_deref().unwrap_or_default(), location)
    }

    /// Like [`Sources::of`], without waiting for the plug-ins: for the UI
    /// thread, which only asks how a location behaves.
    pub fn of_now(&self, location: &str) -> Arc<dyn Source> {
        self.pick(self.plugins.lock().as_deref().unwrap_or_default(), location)
    }

    fn pick(&self, plugins: &[Arc<dyn Source>], location: &str) -> Arc<dyn Source> {
        let plugin = plugins.iter().find(|s| s.handles(location)).cloned();
        plugin.or_else(|| self.built_in.iter().find(|s| s.handles(location)).cloned()).unwrap_or_else(|| Arc::new(FileSource))
    }
}

impl Default for Sources {
    fn default() -> Self {
        Sources {
            plugins: Mutex::new(Some(Vec::new())),
            loaded: Condvar::new(),
            built_in: vec![Arc::new(HttpSource), Arc::new(CueSource), Arc::new(FileSource)],
        }
    }
}

/// Streams and playlists over HTTP(S).
pub struct HttpSource;

impl Source for HttpSource {
    fn id(&self) -> &str {
        "http"
    }

    fn handles(&self, location: &str) -> bool {
        is_url(location)
    }

    fn expand(&self, location: &str, cx: &Context) -> Result<Option<Vec<Item>>, String> {
        if !is_playlist(location) {
            return Ok(None);
        }
        let text = crate::stream::fetch_text(location, &cx.proxy)?;
        let fmt = plformats::PlaylistFormat::from_path(Path::new(location.split('?').next().unwrap_or(location)))
            .or_else(|| plformats::sniff(&text))
            .unwrap_or(plformats::PlaylistFormat::M3u);
        Ok(Some(plformats::parse(&text, fmt, None)))
    }

    fn describe(&self, _location: &str) -> Option<Result<TrackMeta, String>> {
        None
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        Ok(Playback::Http(location.to_string()))
    }
}

/// CUE sheets: a sheet lists its tracks, and each plays a part of a file.
pub struct CueSource;

impl CueSource {
    fn track(location: &str) -> Result<(cue::Sheet, cue::Track), String> {
        let (sheet_path, number) = cue::split_location(location).ok_or("not a track of a CUE sheet")?;
        let sheet = cue::read(Path::new(sheet_path)).map_err(|e| format!("{sheet_path}: {e}"))?;
        let track = sheet.track(number).cloned().ok_or_else(|| format!("{sheet_path} has no track {number}"))?;
        Ok((sheet, track))
    }

    /// The track's length; the last one of a file runs to the file's end.
    fn length_ms(track: &cue::Track, file_length_ms: Option<u64>) -> Option<u64> {
        track.end_ms.or(file_length_ms).map(|end| end.saturating_sub(track.start_ms))
    }

    fn file_length_ms(track: &cue::Track) -> Option<u64> {
        crate::formats::read_meta(&track.file).ok().and_then(|m| m.length_ms)
    }

    /// The file's tags and stream, with what the sheet says of the track.
    fn meta(sheet: &cue::Sheet, track: &cue::Track) -> TrackMeta {
        let mut meta = crate::formats::read_meta(&track.file).unwrap_or_default();
        meta.length_ms = Self::length_ms(track, meta.length_ms);
        meta.title = track.title.clone().or(meta.title);
        meta.artist = track.performer.clone().or(sheet.performer.clone()).or(meta.artist);
        meta.album = sheet.title.clone().or(meta.album);
        meta.album_artist = sheet.performer.clone().or(meta.album_artist);
        meta.track = Some(track.number.to_string());
        // The file's own track gain is the whole file's.
        meta.replaygain_track_gain = None;
        meta.replaygain_track_peak = None;
        meta
    }
}

impl Source for CueSource {
    fn id(&self) -> &str {
        "cue"
    }

    fn handles(&self, location: &str) -> bool {
        !is_url(location) && (cue::is_cue(location) || cue::split_location(location).is_some())
    }

    fn expand(&self, location: &str, _cx: &Context) -> Result<Option<Vec<Item>>, String> {
        if !cue::is_cue(location) {
            return Ok(None);
        }
        let sheet = cue::read(Path::new(location)).map_err(|e| format!("{location}: {e}"))?;
        let items = sheet.tracks.iter().map(|t| {
            let performer = t.performer.as_ref().or(sheet.performer.as_ref());
            let title = match (performer, &t.title) {
                (Some(p), Some(t)) => Some(format!("{p} - {t}")),
                (None, t) => t.clone(),
                (Some(_), None) => None,
            };
            let file_length = if t.end_ms.is_none() { Self::file_length_ms(t) } else { None };
            Item { location: cue::track_location(location, t.number), title, length_ms: Self::length_ms(t, file_length) }
        });
        Ok(Some(items.collect()))
    }

    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>> {
        Some(Self::track(location).map(|(sheet, track)| Self::meta(&sheet, &track)))
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        let (sheet, track) = Self::track(location)?;
        let meta = Box::new(Self::meta(&sheet, &track));
        Ok(Playback::Part { path: track.file, start_ms: track.start_ms, end_ms: track.end_ms, meta })
    }
}

/// Local files, folders and playlist files.
pub struct FileSource;

impl Source for FileSource {
    fn id(&self) -> &str {
        "file"
    }

    fn handles(&self, location: &str) -> bool {
        !is_url(location)
    }

    fn expand(&self, location: &str, _cx: &Context) -> Result<Option<Vec<Item>>, String> {
        let path = Path::new(location);
        if path.is_dir() {
            let mut files = Vec::new();
            walk(path, &mut files, 0);
            files.sort_by(|a, b| text::natural_cmp(a, b));
            return Ok(Some(files.into_iter().map(|f| Item { location: f, title: None, length_ms: None }).collect()));
        }
        if is_playlist(location) {
            return plformats::read_file(path).map(Some).map_err(|e| e.to_string());
        }
        Ok(None)
    }

    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>> {
        Some(crate::formats::read_meta(Path::new(location)))
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        Ok(Playback::File(PathBuf::from(location)))
    }
}

fn walk(dir: &Path, out: &mut Vec<String>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() || (ft.is_symlink() && p.is_dir()) {
            walk(&p, out, depth + 1);
        } else {
            let s = p.to_string_lossy().into_owned();
            if is_audio_file(&s) {
                out.push(s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source that takes `test:` locations.
    struct Test;

    impl Source for Test {
        fn id(&self) -> &str {
            "test"
        }
        fn handles(&self, location: &str) -> bool {
            location.starts_with("test:")
        }
        fn expand(&self, _: &str, _: &Context) -> Result<Option<Vec<Item>>, String> {
            Ok(None)
        }
        fn describe(&self, _: &str) -> Option<Result<TrackMeta, String>> {
            None
        }
        fn open(&self, _: &str, _: &Context) -> Result<Playback, String> {
            Err("test".into())
        }
    }

    #[test]
    fn each_location_goes_to_its_source_and_added_ones_are_asked_first() {
        let sources = Sources::default();
        assert_eq!(sources.of("test:1").id(), "file");
        sources.set_plugins(vec![Arc::new(Test)]);
        for (loc, id) in [
            ("test:1", "test"),
            ("http://radio:8000/stream", "http"),
            ("https://example.com/list.m3u", "http"),
            ("/music/a.mp3", "file"),
            ("relative/b.flac", "file"),
            ("/music/live.cue", "cue"),
            ("/music/live.cue#2", "cue"),
        ] {
            assert_eq!(sources.of(loc).id(), id, "{loc}");
        }
        sources.set_plugins(Vec::new());
        assert_eq!(sources.of("test:1").id(), "file");
    }

    #[test]
    fn a_track_opened_while_plugins_load_waits_for_them() {
        let sources = Arc::new(Sources::waiting());
        assert_eq!(sources.of_now("test:1").id(), "file", "the UI does not wait");
        let asker = {
            let sources = sources.clone();
            std::thread::spawn(move || sources.of("test:1").id().to_string())
        };
        std::thread::sleep(std::time::Duration::from_millis(100));
        sources.set_plugins(vec![Arc::new(Test)]);
        assert_eq!(asker.join().unwrap(), "test");
    }

    #[test]
    fn a_folder_expands_to_its_audio_files_in_natural_order() {
        let dir = std::env::temp_dir().join(format!("popamp-src-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in ["10.mp3", "9.mp3", "notes.txt", "sub/1.flac"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let items = FileSource.expand(&dir.to_string_lossy(), &Context::default()).unwrap().unwrap();
        let names: Vec<_> = items.iter().map(|i| Path::new(&i.location).strip_prefix(&dir).unwrap().to_path_buf()).collect();
        assert_eq!(names, [Path::new("9.mp3"), Path::new("10.mp3"), &Path::new("sub").join("1.flac")]);
        assert_eq!(FileSource.expand(&dir.join("9.mp3").to_string_lossy(), &Context::default()), Ok(None));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_cue_sheet_lists_its_tracks_and_each_plays_its_part() {
        let dir = std::env::temp_dir().join(format!("popamp-cue-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/silence.wav");
        std::fs::copy(&fixture, dir.join("album.wav")).unwrap();
        let file_ms = crate::read_meta(&dir.join("album.wav")).unwrap().length_ms.unwrap();
        let sheet = "PERFORMER \"Band\"\nTITLE \"Album\"\nFILE \"album.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:00:15\n";
        let cue = dir.join("album.cue");
        std::fs::write(&cue, sheet).unwrap();
        let cue = cue.to_string_lossy().into_owned();
        let items = CueSource.expand(&cue, &Context::default()).unwrap().unwrap();
        let second = format!("{cue}#2");
        assert_eq!(items.iter().map(|i| i.location.as_str()).collect::<Vec<_>>(), [format!("{cue}#1").as_str(), second.as_str()]);
        assert_eq!((items[0].title.as_deref(), items[0].length_ms), (Some("Band - One"), Some(200)));
        assert_eq!(items[1].length_ms, Some(file_ms - 200), "the last track runs to the end");
        let meta = CueSource.describe(&second).unwrap().unwrap();
        assert_eq!(
            (meta.title.as_deref(), meta.artist.as_deref(), meta.album.as_deref(), meta.track.as_deref()),
            (Some("Two"), Some("Band"), Some("Album"), Some("2"))
        );
        assert_eq!(
            CueSource.open(&second, &Context::default()),
            Ok(Playback::Part { path: dir.join("album.wav"), start_ms: 200, end_ms: None, meta: Box::new(meta) })
        );
        assert!(CueSource.open(&format!("{cue}#9"), &Context::default()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stream_that_is_not_a_playlist_is_one_track() {
        assert_eq!(HttpSource.expand("http://radio:8000/stream", &Context::default()), Ok(None));
        assert_eq!(HttpSource.open("http://radio:8000/stream", &Context::default()), Ok(Playback::Http("http://radio:8000/stream".into())));
    }
}
