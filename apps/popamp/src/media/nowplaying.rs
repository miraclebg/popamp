//! macOS's Now Playing and media keys: MediaPlayer's Now Playing centre
//! shows the track, its cover and where it is, in Control Center and on the
//! lock screen, and its command centre hands the media keys, AirPods and
//! Control Center's buttons to Popamp while it is the player macOS chose.
//! Fed by the same snapshots as MPRIS on Linux.

use super::{Covers, Snapshot};
use crate::cli::Remote;
use crate::model::AppEvent;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_app_kit::NSImage;
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemArtwork, MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyArtist,
    MPMediaItemPropertyArtwork, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand,
    MPRemoteCommandCenter, MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use parking_lot::Mutex;
use popamp_audio::Shared;
use popamp_core::Command;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;
use winit::event_loop::EventLoopProxy;

/// What Now Playing shows of a snapshot.
#[derive(Debug, Clone, PartialEq)]
struct Info {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    /// None for a stream, which cannot be sought in.
    length_s: Option<f64>,
    elapsed_s: f64,
    /// 1 while playing, 0 otherwise: macOS moves the time on by it.
    rate: f64,
}

/// What `snap` shows, `position_ms` into its track; none without a track.
fn info(snap: &Snapshot, position_ms: u64) -> Option<Info> {
    let track = snap.track.as_ref()?;
    Some(Info {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        length_s: track.length_ms.map(|ms| ms as f64 / 1000.0),
        elapsed_s: position_ms as f64 / 1000.0,
        rate: if snap.status == "Playing" { 1.0 } else { 0.0 },
    })
}

/// A command from the media keys or Control Center.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mac {
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    /// To this many seconds into the track.
    Seek(f64),
}

/// What `command` asks of a player that is `status`, as its buttons would.
fn remotes(command: Mac, status: &str) -> Vec<Remote> {
    let cmd = |c| vec![Remote::Cmd(c)];
    match command {
        Mac::Play => cmd(Command::Play),
        Mac::Pause => super::pause(status).map(Remote::Cmd).into_iter().collect(),
        Mac::Toggle if status == "Playing" => cmd(Command::Pause),
        Mac::Toggle => cmd(Command::Play),
        Mac::Stop => cmd(Command::Stop),
        Mac::Next => cmd(Command::Next),
        Mac::Previous => cmd(Command::Previous),
        Mac::Seek(s) => vec![Remote::SeekMs((s.max(0.0) * 1000.0).round() as u64)],
    }
}

/// Answers the command centre's commands and feeds Now Playing. Called on
/// the main thread, where the handlers are registered.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, _instance: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
    let status = Arc::new(Mutex::new("Stopped"));
    // SAFETY: MediaPlayer's shared command centre.
    let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
    let seek = listen(&center, &proxy, &status);
    let (tx, rx) = crossbeam_channel::unbounded::<Snapshot>();
    let run = move || {
        let mut covers = Covers::default();
        let mut last: Option<Snapshot> = None;
        loop {
            // A new snapshot, or every two seconds the same one, so a seek
            // shows: Now Playing moves the time on by itself between them.
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(snap) => last = Some(snap),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
            let Some(snap) = &last else { continue };
            *status.lock() = snap.status;
            publish(snap, shared.position_ms(), &mut covers, &seek);
        }
        // SAFETY: clearing what Popamp put up.
        unsafe { MPNowPlayingInfoCenter::defaultCenter().setNowPlayingInfo(None) };
    };
    match std::thread::Builder::new().name("popamp-nowplaying".into()).spawn(run) {
        Ok(_) => Some(tx),
        Err(e) => {
            log::warn!("no Now Playing: {e}");
            None
        }
    }
}

/// The seek command, enabled only for a track with a length.
struct Seek(Retained<MPRemoteCommand>);

// SAFETY: MPRemoteCommand's `enabled` may be set from any thread.
unsafe impl Send for Seek {}

/// Registers a handler for each command; they stay for the program's life,
/// as the media service does.
fn listen(center: &MPRemoteCommandCenter, proxy: &EventLoopProxy<AppEvent>, status: &Arc<Mutex<&'static str>>) -> Seek {
    let on = |command: Retained<MPRemoteCommand>, mac: fn(&MPRemoteCommandEvent) -> Mac| {
        let (proxy, status) = (proxy.clone(), status.clone());
        let handler = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
            // SAFETY: the event the command centre passes, alive for the call.
            let event = unsafe { event.as_ref() };
            super::send(&proxy, remotes(mac(event), &status.lock()));
            MPRemoteCommandHandlerStatus::Success
        });
        // SAFETY: a handler block of the type the command centre asks for.
        let target = unsafe { command.addTargetWithHandler(&handler) };
        std::mem::forget(target);
        command
    };
    // SAFETY: the command centre's commands.
    unsafe {
        on(center.playCommand(), |_| Mac::Play);
        on(center.pauseCommand(), |_| Mac::Pause);
        on(center.togglePlayPauseCommand(), |_| Mac::Toggle);
        on(center.stopCommand(), |_| Mac::Stop);
        on(center.nextTrackCommand(), |_| Mac::Next);
        on(center.previousTrackCommand(), |_| Mac::Previous);
        let seek = on(center.changePlaybackPositionCommand().into_super(), |event| {
            // SAFETY: this command's events are position changes.
            let event: &MPChangePlaybackPositionCommandEvent = &*(event as *const MPRemoteCommandEvent).cast();
            Mac::Seek(event.positionTime())
        });
        Seek(seek)
    }
}

/// Puts `snap` up in Now Playing, `position_ms` into its track.
fn publish(snap: &Snapshot, position_ms: u64, covers: &mut Covers, seek: &Seek) {
    // SAFETY: MediaPlayer's shared Now Playing centre; its setters may be
    // called from any thread.
    let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
    let state = match snap.status {
        "Playing" => MPNowPlayingPlaybackState::Playing,
        "Paused" => MPNowPlayingPlaybackState::Paused,
        _ => MPNowPlayingPlaybackState::Stopped,
    };
    let Some(i) = info(snap, position_ms) else {
        // SAFETY: as above.
        unsafe {
            center.setNowPlayingInfo(None);
            center.setPlaybackState(state);
        }
        return;
    };
    // SAFETY: setting `enabled` on a command.
    unsafe { seek.0.setEnabled(i.length_s.is_some()) };
    let text = |s: &str| -> Retained<AnyObject> { NSString::from_str(s).into() };
    let number = |n: f64| -> Retained<AnyObject> { NSNumber::new_f64(n).into() };
    // SAFETY: MediaPlayer's property names, constant strings.
    let mut entries: Vec<(&NSString, Retained<AnyObject>)> = unsafe {
        vec![
            (MPMediaItemPropertyTitle, text(&i.title)),
            (MPNowPlayingInfoPropertyElapsedPlaybackTime, number(i.elapsed_s)),
            (MPNowPlayingInfoPropertyPlaybackRate, number(i.rate)),
        ]
    };
    // SAFETY: as above.
    unsafe {
        if let Some(artist) = &i.artist {
            entries.push((MPMediaItemPropertyArtist, text(artist)));
        }
        if let Some(album) = &i.album {
            entries.push((MPMediaItemPropertyAlbumTitle, text(album)));
        }
        if let Some(length) = i.length_s {
            entries.push((MPMediaItemPropertyPlaybackDuration, number(length)));
        }
        if let Some(art) = snap.track.as_ref().and_then(|t| covers.of(t)).and_then(|p| artwork(&p.to_string_lossy())) {
            entries.push((MPMediaItemPropertyArtwork, art));
        }
    }
    let (keys, values): (Vec<&NSString>, Vec<Retained<AnyObject>>) = entries.into_iter().unzip();
    let values: Vec<&AnyObject> = values.iter().map(|v| &**v).collect();
    let dict = NSDictionary::from_slices(&keys, &values);
    // SAFETY: a dictionary of Now Playing's keys and values of their types.
    unsafe {
        center.setNowPlayingInfo(Some(&dict));
        center.setPlaybackState(state);
    }
}

/// The cover at `path` as Now Playing's artwork.
fn artwork(path: &str) -> Option<Retained<AnyObject>> {
    let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))?;
    let size = image.size();
    let handler = RcBlock::new(move |_: CGSize| NonNull::from(&*image));
    // SAFETY: an image the block keeps alive, at its own size.
    let art = unsafe { MPMediaItemArtwork::initWithBoundsSize_requestHandler(MPMediaItemArtwork::alloc(), size, &handler) };
    Some(art.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Remote;
    use crate::media::Track;
    use popamp_core::Command;

    fn snap(status: &'static str, length_ms: Option<u64>) -> Snapshot {
        let track = Track { id: 1, title: "Song".into(), artist: Some("Band".into()), album: None, url: "/a.mp3".into(), length_ms };
        Snapshot { status, track: Some(track), ..Snapshot::default() }
    }

    #[test]
    fn now_playing_shows_the_track_where_it_is() {
        let i = info(&snap("Playing", Some(200_000)), 61_500).unwrap();
        assert_eq!(
            (i.title.as_str(), i.artist.as_deref(), i.length_s, i.elapsed_s, i.rate),
            ("Song", Some("Band"), Some(200.0), 61.5, 1.0)
        );
        assert_eq!(info(&snap("Paused", Some(200_000)), 0).unwrap().rate, 0.0);
        assert!(info(&Snapshot::default(), 0).is_none());
    }

    #[test]
    fn a_stream_has_no_length_to_seek_in() {
        assert_eq!(info(&snap("Playing", None), 5_000).unwrap().length_s, None);
    }

    #[test]
    fn the_media_keys_ask_what_the_buttons_ask() {
        assert_eq!(remotes(Mac::Toggle, "Playing"), [Remote::Cmd(Command::Pause)]);
        assert_eq!(remotes(Mac::Toggle, "Stopped"), [Remote::Cmd(Command::Play)]);
        assert_eq!(remotes(Mac::Toggle, "Paused"), [Remote::Cmd(Command::Play)]);
        assert!(remotes(Mac::Pause, "Paused").is_empty(), "pausing a paused player would resume it");
        assert_eq!(remotes(Mac::Seek(12.25), "Playing"), [Remote::SeekMs(12_250)]);
        assert_eq!(remotes(Mac::Next, "Playing"), [Remote::Cmd(Command::Next)]);
    }
}
