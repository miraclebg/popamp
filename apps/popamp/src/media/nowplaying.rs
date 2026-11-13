//! macOS's Now Playing and media keys: MediaPlayer's Now Playing centre
//! shows the track, its cover and where it is, in Control Center and on the
//! lock screen, and its command centre hands the media keys, AirPods and
//! Control Center's buttons to Popamp while it is the player macOS chose.
//! Fed by the same snapshots as MPRIS on Linux. MediaPlayer is touched only
//! on the main thread: the service's thread works out what to show and
//! hands it to the main queue.

use super::{Covers, Snapshot};
use crate::cli::Remote;
use crate::model::AppEvent;
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_app_kit::NSImage;
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemArtwork, MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyArtist,
    MPMediaItemPropertyArtwork, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle, MPNowPlayingInfoCenter,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyIsLiveStream, MPNowPlayingInfoPropertyPlaybackRate,
    MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter, MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use parking_lot::Mutex;
use popamp_audio::Shared;
use popamp_core::Command;
use std::cell::RefCell;
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
/// Play/Pause is left to the UI thread, which knows the true state; Play
/// while playing would restart the song, and pausing a paused player would
/// resume it, so those ask nothing.
fn remotes(command: Mac, status: &str) -> Vec<Remote> {
    let cmd = |c| vec![Remote::Cmd(c)];
    match command {
        Mac::Play if status == "Playing" => Vec::new(),
        Mac::Play => cmd(Command::Play),
        Mac::Pause => super::pause(status).map(Remote::Cmd).into_iter().collect(),
        Mac::Toggle => cmd(Command::PlayPause),
        Mac::Stop => cmd(Command::Stop),
        Mac::Next => cmd(Command::Next),
        Mac::Previous => cmd(Command::Previous),
        Mac::Seek(s) => vec![Remote::SeekMs((s.max(0.0) * 1000.0).round() as u64)],
    }
}

/// What the main thread puts up: the track as `Info`, the state, and the
/// track's id with its cover file, if it has one.
struct Update {
    info: Option<Info>,
    state: MPNowPlayingPlaybackState,
    cover: Option<(u64, Option<String>)>,
}

thread_local! {
    /// The seek command, enabled only for a track with a length.
    static SEEK: RefCell<Option<Retained<MPRemoteCommand>>> = const { RefCell::new(None) };
    /// The artwork of the track on show, made once per track.
    static ARTWORK: RefCell<Option<(u64, Option<Retained<AnyObject>>)>> = const { RefCell::new(None) };
}

/// Answers the command centre's commands and feeds Now Playing. Called on
/// the main thread, where the handlers are registered.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, _instance: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
    let status = Arc::new(Mutex::new("Stopped"));
    // SAFETY: MediaPlayer's shared command centre, on the main thread.
    let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
    let seek = listen(&center, &proxy, &status);
    SEEK.with(|s| *s.borrow_mut() = Some(seek));
    let (tx, rx) = crossbeam_channel::unbounded::<Snapshot>();
    let run = move || {
        let mut covers = Covers::default();
        let mut cover: Option<(u64, Option<String>)> = None;
        let mut last: Option<Snapshot> = None;
        loop {
            // A new snapshot, or, while playing, every two seconds the same
            // one, so a seek shows: Now Playing moves the time on by itself.
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(snap) => last = Some(snap),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) if last.as_ref().is_some_and(|s| s.status == "Playing") => {}
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
            let Some(snap) = &last else { continue };
            *status.lock() = snap.status;
            // The cover is looked up once per track, as MPRIS does.
            let id = snap.track.as_ref().map(|t| t.id);
            if cover.as_ref().map(|c| c.0) != id {
                cover = snap.track.as_ref().map(|t| (t.id, covers.of(t).map(|p| p.to_string_lossy().into_owned())));
            }
            let update = Update { info: info(snap, shared.position_ms()), state: state(snap.status), cover: cover.clone() };
            DispatchQueue::main().exec_async(move || publish(update));
        }
        DispatchQueue::main().exec_async(|| {
            // SAFETY: clearing what Popamp put up, on the main thread.
            unsafe {
                let center = MPNowPlayingInfoCenter::defaultCenter();
                center.setNowPlayingInfo(None);
                center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            }
        });
    };
    match std::thread::Builder::new().name("popamp-nowplaying".into()).spawn(run) {
        Ok(_) => Some(tx),
        Err(e) => {
            log::warn!("no Now Playing: {e}");
            None
        }
    }
}

/// Now Playing's state for a player that is `status`.
fn state(status: &str) -> MPNowPlayingPlaybackState {
    match status {
        "Playing" => MPNowPlayingPlaybackState::Playing,
        "Paused" => MPNowPlayingPlaybackState::Paused,
        _ => MPNowPlayingPlaybackState::Stopped,
    }
}

/// Registers a handler for each command and gives back the seek command;
/// they stay for the program's life, as the media service does.
fn listen(
    center: &MPRemoteCommandCenter,
    proxy: &EventLoopProxy<AppEvent>,
    status: &Arc<Mutex<&'static str>>,
) -> Retained<MPRemoteCommand> {
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
    // SAFETY: the command centre's commands, on the main thread.
    unsafe {
        on(center.playCommand(), |_| Mac::Play);
        on(center.pauseCommand(), |_| Mac::Pause);
        on(center.togglePlayPauseCommand(), |_| Mac::Toggle);
        on(center.stopCommand(), |_| Mac::Stop);
        on(center.nextTrackCommand(), |_| Mac::Next);
        on(center.previousTrackCommand(), |_| Mac::Previous);
        on(center.changePlaybackPositionCommand().into_super(), |event| {
            // SAFETY: this command's events are position changes.
            let event: &MPChangePlaybackPositionCommandEvent = &*(event as *const MPRemoteCommandEvent).cast();
            Mac::Seek(event.positionTime())
        })
    }
}

/// Puts `update` up in Now Playing. On the main thread.
fn publish(update: Update) {
    // SAFETY: MediaPlayer's shared Now Playing centre, on the main thread.
    let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
    let Some(i) = update.info else {
        // SAFETY: as above.
        unsafe {
            center.setNowPlayingInfo(None);
            center.setPlaybackState(update.state);
        }
        return;
    };
    // SAFETY: the seek command registered on this thread.
    SEEK.with(|s| s.borrow().as_ref().map(|seek| unsafe { seek.setEnabled(i.length_s.is_some()) }));
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
        match i.length_s {
            Some(length) => entries.push((MPMediaItemPropertyPlaybackDuration, number(length))),
            // A stream: Control Center says "Live" rather than counting up.
            None => entries.push((MPNowPlayingInfoPropertyIsLiveStream, NSNumber::new_bool(true).into())),
        }
        if let Some(art) = update.cover.and_then(artwork_of) {
            entries.push((MPMediaItemPropertyArtwork, art));
        }
    }
    let (keys, values): (Vec<&NSString>, Vec<Retained<AnyObject>>) = entries.into_iter().unzip();
    let values: Vec<&AnyObject> = values.iter().map(|v| &**v).collect();
    let dict = NSDictionary::from_slices(&keys, &values);
    // SAFETY: a dictionary of Now Playing's keys and values of their types.
    unsafe {
        center.setNowPlayingInfo(Some(&dict));
        center.setPlaybackState(update.state);
    }
}

/// The artwork of track `id` from its cover file, made the first time the
/// track is shown and kept until another one is.
fn artwork_of((id, path): (u64, Option<String>)) -> Option<Retained<AnyObject>> {
    ARTWORK.with(|a| {
        let mut a = a.borrow_mut();
        if a.as_ref().map(|(shown, _)| *shown) != Some(id) {
            *a = Some((id, path.as_deref().and_then(artwork)));
        }
        a.as_ref().and_then(|(_, art)| art.clone())
    })
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
        // Play/Pause is decided on the UI thread from the true state, so
        // two quick presses never restart the song.
        assert_eq!(remotes(Mac::Toggle, "Playing"), [Remote::Cmd(Command::PlayPause)]);
        assert_eq!(remotes(Mac::Toggle, "Stopped"), [Remote::Cmd(Command::PlayPause)]);
        assert!(remotes(Mac::Play, "Playing").is_empty(), "Play while playing would restart the song");
        assert_eq!(remotes(Mac::Play, "Paused"), [Remote::Cmd(Command::Play)]);
        assert!(remotes(Mac::Pause, "Paused").is_empty(), "pausing a paused player would resume it");
        assert_eq!(remotes(Mac::Seek(12.25), "Playing"), [Remote::SeekMs(12_250)]);
        assert_eq!(remotes(Mac::Next, "Playing"), [Remote::Cmd(Command::Next)]);
    }
}
