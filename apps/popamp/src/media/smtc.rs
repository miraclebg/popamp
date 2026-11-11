//! System Media Transport Controls, the media controls on Windows: the
//! media keys and the player card by the volume flyout.
//!
//! The controls are those of a media player object of our own with its
//! own handling off: it plays nothing. A thread of ours makes them, shows
//! each [`Snapshot`] and, while a track plays, its position; what is asked
//! of them arrives on the system's threads and goes to the UI thread as
//! remote commands. The player puts a hidden window on that thread, so
//! the thread answers window messages while it waits.

use super::{Covers, Snapshot, Track};
use crate::cli::Remote;
use crate::model::AppEvent;
use crossbeam_channel::{RecvTimeoutError, TryRecvError};
use parking_lot::Mutex;
use popamp_audio::Shared;
use popamp_core::Command;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{Interface, Ref, Result, RuntimeType, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    AutoRepeatModeChangeRequestedEventArgs, MediaPlaybackAutoRepeatMode as Repeat, MediaPlaybackStatus, MediaPlaybackType,
    PlaybackPositionChangeRequestedEventArgs, ShuffleEnabledChangeRequestedEventArgs, SystemMediaTransportControls,
    SystemMediaTransportControlsButton as Button, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsDisplayUpdater, SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::StorageFile;
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use winit::event_loop::EventLoopProxy;

/// How often the position of a playing track is shown again.
const TICK: Duration = Duration::from_secs(1);

/// The longest a new snapshot waits to be shown.
const POLL: Duration = Duration::from_millis(50);

/// A [`TimeSpan`] counts 100-nanosecond ticks.
const TICKS_PER_MS: i64 = 10_000;

/// The player as shown to the system's last snapshot, read by the button
/// handler.
type Status = Arc<Mutex<&'static str>>;

/// Starts the service; `None` if this Windows has no media controls.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, _: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
    let (tx, rx) = crossbeam_channel::unbounded::<Snapshot>();
    let (ready, opened) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name("popamp-smtc".into())
        .spawn(move || {
            // SAFETY: once for this thread, before any other COM call on it.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let status: Status = Arc::new(Mutex::new(Snapshot::default().status));
            match open(&proxy, status.clone()) {
                Ok((_player, controls)) => {
                    let _ = ready.send(true);
                    // Wine's controls are the first version's: buttons and the card.
                    let more = extend(&controls, &proxy).inspect_err(|e| log::info!("media controls without a timeline: {e}")).is_ok();
                    run(&mut Card { controls, more, shown: None, covers: Covers::default() }, &rx, &status, &shared);
                }
                Err(e) => {
                    log::warn!("no media controls: {e}");
                    let _ = ready.send(false);
                }
            }
        })
        .ok()?;
    opened.recv().ok()?.then_some(tx)
}

/// The controls; `more` when they have the timeline, shuffle and repeat.
struct Card {
    controls: SystemMediaTransportControls,
    more: bool,
    /// The track on the card, once one was put there.
    shown: Option<Option<Track>>,
    covers: Covers,
}

/// Shows each snapshot from `rx`, and the position every [`TICK`] while
/// playing, until the UI thread is gone.
fn run(card: &mut Card, rx: &crossbeam_channel::Receiver<Snapshot>, status: &Status, shared: &Shared) {
    let mut snap = Snapshot::default();
    loop {
        let res = match recv(rx, TICK) {
            Ok(new) => {
                *status.lock() = new.status;
                snap = new;
                show(card, &snap)
            }
            Err(RecvTimeoutError::Timeout) if snap.status == "Playing" && card.more => Ok(()),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let length = snap.track.as_ref().and_then(|t| t.length_ms);
        let res = res.and_then(|()| if card.more { timeline(&card.controls, length, shared.position_ms()) } else { Ok(()) });
        if let Err(e) = res {
            log::debug!("media controls: {e}");
        }
    }
}

/// Waits up to `timeout` for a snapshot, answering this thread's window
/// messages meanwhile. A window whose thread does not answer holds up
/// whoever sends it a message: Explorer dropping files, the input method,
/// and with them the UI thread.
fn recv(rx: &crossbeam_channel::Receiver<Snapshot>, timeout: Duration) -> std::result::Result<Snapshot, RecvTimeoutError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MsgWaitForMultipleObjectsEx, MWMO_INPUTAVAILABLE, QS_ALLINPUT};
    let until = Instant::now() + timeout;
    loop {
        pump();
        match rx.try_recv() {
            Ok(snap) => return Ok(snap),
            Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
            Err(TryRecvError::Empty) => {}
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(RecvTimeoutError::Timeout);
        }
        let wait = left.min(POLL).as_millis() as u32;
        // SAFETY: no handles to wait on, only this thread's message queue.
        unsafe { MsgWaitForMultipleObjectsEx(0, std::ptr::null(), wait, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
    }
}

/// Handles the messages waiting for this thread's windows.
fn pump() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
    // SAFETY: `msg` is ours to fill; a null window takes every message of this thread.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The controls, answering their buttons; the player they belong to has
/// to live as long.
fn open(proxy: &EventLoopProxy<AppEvent>, status: Status) -> Result<(MediaPlayer, SystemMediaTransportControls)> {
    let player = MediaPlayer::new()?;
    quiet(&player)?;
    let controls = player.SystemMediaTransportControls()?;
    type Switch = fn(&SystemMediaTransportControls, bool) -> Result<()>;
    let on: [Switch; 6] = [
        SystemMediaTransportControls::SetIsEnabled,
        SystemMediaTransportControls::SetIsPlayEnabled,
        SystemMediaTransportControls::SetIsPauseEnabled,
        SystemMediaTransportControls::SetIsStopEnabled,
        SystemMediaTransportControls::SetIsNextEnabled,
        SystemMediaTransportControls::SetIsPreviousEnabled,
    ];
    for switch in on {
        switch(&controls, true)?;
    }
    controls.ButtonPressed(&handler(proxy, move |a: &SystemMediaTransportControlsButtonPressedEventArgs| {
        Some(Remote::Cmd(command(a.Button().ok()?, *status.lock())?))
    }))?;
    Ok((player, controls))
}

/// Stops `player` answering the controls itself; Wine's player has no
/// command manager, nor handling of its own to stop.
fn quiet(player: &MediaPlayer) -> Result<()> {
    match player.CommandManager() {
        Ok(manager) => manager.SetIsEnabled(false),
        Err(_) => Ok(()),
    }
}

/// Has the controls answer seeking, shuffle and repeat too.
fn extend(controls: &SystemMediaTransportControls, proxy: &EventLoopProxy<AppEvent>) -> Result<()> {
    controls.PlaybackPositionChangeRequested(&handler(proxy, |a: &PlaybackPositionChangeRequestedEventArgs| {
        Some(Remote::SeekMs(ms(a.RequestedPlaybackPosition().ok()?)))
    }))?;
    controls.ShuffleEnabledChangeRequested(&handler(proxy, |a: &ShuffleEnabledChangeRequestedEventArgs| {
        Some(Remote::Shuffle(Some(a.RequestedShuffleEnabled().ok()?)))
    }))?;
    controls.AutoRepeatModeChangeRequested(&handler(proxy, |a: &AutoRepeatModeChangeRequestedEventArgs| {
        Some(Remote::Repeat(Some(a.RequestedAutoRepeatMode().ok()? != Repeat::None)))
    }))?;
    Ok(())
}

/// A handler for an event of the controls that sends what `remote` makes
/// of its arguments to the UI thread.
fn handler<A>(
    proxy: &EventLoopProxy<AppEvent>,
    remote: impl Fn(&A) -> Option<Remote> + Send + 'static,
) -> TypedEventHandler<SystemMediaTransportControls, A>
where
    A: Interface + RuntimeType + 'static,
{
    let proxy = proxy.clone();
    TypedEventHandler::new(move |_, args: Ref<A>| {
        if let Some(r) = args.as_ref().and_then(&remote) {
            super::send(&proxy, vec![r]);
        }
        Ok(())
    })
}

/// Shows `snap`; the track only when it is not on the card already.
fn show(card: &mut Card, snap: &Snapshot) -> Result<()> {
    let controls = &card.controls;
    controls.SetPlaybackStatus(playback(snap.status))?;
    if card.more {
        controls.SetShuffleEnabled(snap.shuffle)?;
        controls.SetAutoRepeatMode(if snap.repeat { Repeat::List } else { Repeat::None })?;
    }
    if card.shown.as_ref() == Some(&snap.track) {
        return Ok(());
    }
    put(card, snap.track.as_ref())?;
    card.shown = Some(snap.track.clone());
    Ok(())
}

/// Puts `track`, with its cover, on the card in place of what was there.
fn put(card: &mut Card, track: Option<&Track>) -> Result<()> {
    let display = card.controls.DisplayUpdater()?;
    display.ClearAll()?;
    if let Some(t) = track {
        describe(&display, t)?;
        if let Some(path) = card.covers.of(t) {
            if let Err(e) = thumbnail(&display, &path) {
                log::debug!("cover {}: {e}", path.display());
            }
        }
    }
    display.Update()
}

/// Puts the image file at `path` on the card.
fn thumbnail(display: &SystemMediaTransportControlsDisplayUpdater, path: &Path) -> Result<()> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.join()?;
    display.SetThumbnail(&RandomAccessStreamReference::CreateFromFile(&file)?)
}

/// Puts `track` on the player card.
fn describe(display: &SystemMediaTransportControlsDisplayUpdater, track: &Track) -> Result<()> {
    display.SetType(MediaPlaybackType::Music)?;
    let music = display.MusicProperties()?;
    let text = |s: Option<&String>| HSTRING::from(s.map_or("", |s| s.as_str()));
    music.SetTitle(&text(Some(&track.title)))?;
    music.SetArtist(&text(track.artist.as_ref()))?;
    music.SetAlbumTitle(&text(track.album.as_ref()))
}

/// Shows a track `length_ms` long at `position_ms`; one of no known
/// length, a stream, cannot be sought.
fn timeline(controls: &SystemMediaTransportControls, length_ms: Option<u64>, position_ms: u64) -> Result<()> {
    let end = length_ms.unwrap_or(0);
    let t = SystemMediaTransportControlsTimelineProperties::new()?;
    t.SetStartTime(span(0))?;
    t.SetEndTime(span(end))?;
    t.SetMinSeekTime(span(0))?;
    t.SetMaxSeekTime(span(end))?;
    t.SetPosition(span(position_ms.min(end)))?;
    controls.UpdateTimelineProperties(&t)
}

fn span(ms: u64) -> TimeSpan {
    TimeSpan { Duration: ms as i64 * TICKS_PER_MS }
}

fn ms(span: TimeSpan) -> u64 {
    (span.Duration / TICKS_PER_MS).max(0) as u64
}

/// What `button` asks of a player that is `status`.
fn command(button: Button, status: &str) -> Option<Command> {
    match button {
        Button::Play => Some(Command::Play),
        Button::Pause => super::pause(status),
        Button::Stop => Some(Command::Stop),
        Button::Next => Some(Command::Next),
        Button::Previous => Some(Command::Previous),
        _ => None,
    }
}

fn playback(status: &str) -> MediaPlaybackStatus {
    match status {
        "Playing" => MediaPlaybackStatus::Playing,
        "Paused" => MediaPlaybackStatus::Paused,
        _ => MediaPlaybackStatus::Stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_are_player_commands() {
        assert_eq!(command(Button::Next, "Playing"), Some(Command::Next));
        assert_eq!(command(Button::Previous, "Stopped"), Some(Command::Previous));
        assert_eq!(command(Button::Play, "Paused"), Some(Command::Play));
        assert_eq!(command(Button::Pause, "Paused"), None, "pausing would resume");
        assert_eq!(command(Button::Record, "Playing"), None);
    }

    #[test]
    fn time_spans_count_tenths_of_microseconds() {
        assert_eq!(span(1500).Duration, 15_000_000);
        assert_eq!(ms(span(83_250)), 83_250);
        assert_eq!(ms(TimeSpan { Duration: -5 }), 0, "before the start is the start");
    }
}
