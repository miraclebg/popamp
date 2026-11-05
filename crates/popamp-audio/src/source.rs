//! Sources: who takes a location, lists it, describes it and says how to
//! play it. Files and HTTP are built in; YouTube and later plug-ins are
//! sources too, asked first. A source never handles audio: it answers
//! with a [`Playback`] and the engine plays.
//!
//! Every call but `handles` may be slow (a network or a program) and runs
//! on a worker thread, never on the engine or UI thread.

use crate::pipe::Pipe;
use parking_lot::RwLock;
use popamp_core::meta::TrackMeta;
use popamp_core::playlist::is_url;
use popamp_core::plformats::{self, is_playlist, Item};
use popamp_core::{is_audio_file, text};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What a call needs from the player's settings.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// HTTP proxy (`[user:pass@]host:port`), empty for none.
    pub proxy: String,
}

/// How the engine plays a track.
#[derive(Debug, Clone, PartialEq)]
pub enum Playback {
    File(PathBuf),
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

/// The sources in the order they are asked: plug-ins, then the ones the
/// player was built with, then HTTP and files. Shared by every thread that
/// asks; plug-ins can be swapped while it runs.
pub struct Sources {
    plugins: RwLock<Vec<Arc<dyn Source>>>,
    built_in: Vec<Arc<dyn Source>>,
}

impl Sources {
    pub fn new(first: Vec<Arc<dyn Source>>) -> Sources {
        let mut built_in = first;
        built_in.push(Arc::new(HttpSource));
        built_in.push(Arc::new(FileSource));
        Sources { plugins: RwLock::new(Vec::new()), built_in }
    }

    /// Replaces the plug-ins asked before the built-in sources.
    pub fn set_plugins(&self, plugins: Vec<Arc<dyn Source>>) {
        *self.plugins.write() = plugins;
    }

    /// The source that takes `location`; anything not a URL is a file.
    pub fn of(&self, location: &str) -> Arc<dyn Source> {
        let plugin = self.plugins.read().iter().find(|s| s.handles(location)).cloned();
        plugin.or_else(|| self.built_in.iter().find(|s| s.handles(location)).cloned()).unwrap_or_else(|| Arc::new(FileSource))
    }
}

impl Default for Sources {
    fn default() -> Self {
        Sources::new(Vec::new())
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
        Some(crate::decoder::read_meta(Path::new(location)))
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
        ] {
            assert_eq!(sources.of(loc).id(), id, "{loc}");
        }
        sources.set_plugins(Vec::new());
        assert_eq!(sources.of("test:1").id(), "file");
    }

    #[test]
    fn a_folder_expands_to_its_audio_files_in_natural_order() {
        let dir = std::env::temp_dir().join(format!("popamp-src-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in ["10.mp3", "9.mp3", "notes.txt", "sub/1.flac"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let items = FileSource.expand(&dir.to_string_lossy(), &Context::default()).unwrap().unwrap();
        let names: Vec<_> = items.iter().map(|i| i.location.strip_prefix(&*dir.to_string_lossy()).unwrap().to_string()).collect();
        assert_eq!(names, ["/9.mp3", "/10.mp3", "/sub/1.flac"]);
        assert_eq!(FileSource.expand(&dir.join("9.mp3").to_string_lossy(), &Context::default()), Ok(None));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stream_that_is_not_a_playlist_is_one_track() {
        assert_eq!(HttpSource.expand("http://radio:8000/stream", &Context::default()), Ok(None));
        assert_eq!(HttpSource.open("http://radio:8000/stream", &Context::default()), Ok(Playback::Http("http://radio:8000/stream".into())));
    }
}
