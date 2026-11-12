//! The desktop's media controls: media keys, desktop widgets, playerctl.
//! The UI thread sends [`Snapshot`]s through [`Controls`]; each system's
//! service turns them into its own notifications and sends method calls
//! back as remote commands. MPRIS2 over D-Bus on Linux, the System Media
//! Transport Controls on Windows, Now Playing on macOS (not yet).

// The services' shared helpers go unused on macOS until Now Playing is there.
#![cfg_attr(target_os = "macos", allow(dead_code))]

use crate::cli::Remote;
use crate::ipc::Request;
use crate::model::AppEvent;
use popamp_audio::cover::{self, Cover};
use popamp_audio::Shared;
use popamp_core::Command;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;

#[cfg(linux_like)]
mod mpris;
#[cfg(linux_like)]
use mpris as service;
#[cfg(target_os = "macos")]
mod nowplaying;
#[cfg(target_os = "macos")]
use nowplaying as service;
#[cfg(windows)]
mod smtc;
#[cfg(windows)]
use smtc as service;

/// Sends `remotes` to the UI thread, as a remote control would.
fn send(proxy: &EventLoopProxy<AppEvent>, remotes: Vec<Remote>) {
    let (tx, _rx) = crossbeam_channel::bounded(1);
    let _ = proxy.send_event(AppEvent::Remote(Request { remotes, reply: tx }));
}

/// What a Pause button asks of a player that is `status`: nothing unless
/// it plays, since pausing a paused player resumes it.
fn pause(status: &str) -> Option<Command> {
    (status == "Playing").then_some(Command::Pause)
}

/// The cover of the track on show as a file the desktop can read: an
/// image beside the track, or the picture from its tags written out to a
/// file of ours, kept until the next track.
#[derive(Default)]
struct Covers {
    written: Option<PathBuf>,
}

impl Covers {
    /// The cover file of `track`, if it has a cover.
    fn of(&mut self, track: &Track) -> Option<PathBuf> {
        self.forget();
        if popamp_core::playlist::is_url(&track.url) {
            return None;
        }
        match cover::find(Path::new(&track.url))? {
            Cover::File(path) => Some(path),
            Cover::Tagged(picture) => {
                let path = std::env::temp_dir().join(format!("popamp-cover-{}-{}.{}", std::process::id(), track.id, extension(&picture)));
                std::fs::write(&path, &picture.data).inspect_err(|e| log::debug!("{}: {e}", path.display())).ok()?;
                self.written = Some(path.clone());
                Some(path)
            }
        }
    }

    fn forget(&mut self) {
        if let Some(path) = self.written.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for Covers {
    fn drop(&mut self) {
        self.forget();
    }
}

/// The file name extension for `picture`'s media type.
fn extension(picture: &cover::Picture) -> &'static str {
    match picture.media_type.as_str() {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/webp" => "webp",
        _ => "jpg",
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub url: String,
    pub length_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub status: &'static str,
    pub repeat: bool,
    pub shuffle: bool,
    pub volume: f64,
    pub track: Option<Track>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self { status: "Stopped", repeat: false, shuffle: false, volume: 1.0, track: None }
    }
}

pub struct Controls {
    tx: crossbeam_channel::Sender<Snapshot>,
    last: Option<Snapshot>,
}

impl Controls {
    /// Publishes the snapshot if it changed.
    pub fn update(&mut self, snap: Snapshot) {
        if self.last.as_ref() != Some(&snap) {
            self.last = Some(snap.clone());
            let _ = self.tx.send(snap);
        }
    }
}

/// Starts the system's service; `None` if there is none or it is not
/// running.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, instance: &str) -> Option<Controls> {
    Some(Controls { tx: service::spawn(proxy, shared, instance)?, last: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u64, url: &str) -> Track {
        Track { id, url: url.into(), ..Track::default() }
    }

    #[test]
    fn a_tagged_cover_is_written_out_until_the_next_track() {
        let dir = std::env::temp_dir().join(format!("popamp-covers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // ID3v2.3 with a 3-byte PNG "cover", then MPEG-1 Layer III frames.
        let mut mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x1a".to_vec();
        mp3.extend_from_slice(b"APIC\x00\x00\x00\x10\x00\x00\x00image/png\x00\x03\x00png");
        for _ in 0..4 {
            mp3.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            mp3.extend_from_slice(&[0; 413]);
        }
        let tagged = dir.join("a.mp3");
        std::fs::write(&tagged, mp3).unwrap();
        let mut covers = Covers::default();
        let written = covers.of(&track(7, tagged.to_str().unwrap())).expect("a cover");
        assert_eq!(written.extension().unwrap(), "png");
        assert_eq!(std::fs::read(&written).unwrap(), b"png");
        assert_eq!(covers.of(&track(8, "http://radio.example/stream")), None);
        assert!(!written.exists(), "gone with its track");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pause_only_pauses() {
        assert_eq!(pause("Playing"), Some(Command::Pause));
        assert_eq!(pause("Paused"), None);
        assert_eq!(pause("Stopped"), None);
    }
}
