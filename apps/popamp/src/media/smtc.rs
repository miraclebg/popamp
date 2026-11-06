//! System Media Transport Controls, the media controls on Windows: the
//! media keys and the player card by the volume flyout.
//!
//! The controls are those of a media player object of our own with its
//! own handling off: it plays nothing and needs no window. A thread of
//! ours makes them and shows each [`Snapshot`]; button presses arrive on
//! the system's threads and go to the UI thread as remote commands.

use super::{Snapshot, Track};
use crate::cli::Remote;
use crate::model::AppEvent;
use parking_lot::Mutex;
use popamp_audio::Shared;
use popamp_core::Command;
use std::sync::Arc;
use windows::core::{Ref, Result, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls, SystemMediaTransportControlsButton as Button,
    SystemMediaTransportControlsButtonPressedEventArgs, SystemMediaTransportControlsDisplayUpdater,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use winit::event_loop::EventLoopProxy;

/// The player as shown to the system's last snapshot, read by the button
/// handler.
type Status = Arc<Mutex<&'static str>>;

/// Starts the service; `None` if this Windows has no media controls.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, _: Arc<Shared>, _: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
    let (tx, rx) = crossbeam_channel::unbounded::<Snapshot>();
    let (ready, opened) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name("popamp-smtc".into())
        .spawn(move || {
            // SAFETY: once for this thread, before any other COM call on it.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let status: Status = Arc::new(Mutex::new(Snapshot::default().status));
            let (_player, controls) = match open(proxy, status.clone()) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("no media controls: {e}");
                    let _ = ready.send(false);
                    return;
                }
            };
            let _ = ready.send(true);
            let mut shown = None;
            for snap in rx {
                *status.lock() = snap.status;
                if let Err(e) = show(&controls, &snap, &mut shown) {
                    log::debug!("media controls: {e}");
                }
            }
        })
        .ok()?;
    opened.recv().ok()?.then_some(tx)
}

/// The controls, with the buttons Popamp answers on; the player they
/// belong to has to live as long.
fn open(proxy: EventLoopProxy<AppEvent>, status: Status) -> Result<(MediaPlayer, SystemMediaTransportControls)> {
    let player = MediaPlayer::new()?;
    // Wine's player has no command manager, nor handling of its own to stop.
    if let Ok(manager) = player.CommandManager() {
        manager.SetIsEnabled(false)?;
    }
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
    let pressed = move |_: Ref<SystemMediaTransportControls>, args: Ref<SystemMediaTransportControlsButtonPressedEventArgs>| {
        if let Some(c) = args.as_ref().and_then(|a| a.Button().ok()).and_then(|b| command(b, *status.lock())) {
            super::send(&proxy, vec![Remote::Cmd(c)]);
        }
        Ok(())
    };
    controls.ButtonPressed(&TypedEventHandler::new(pressed))?;
    Ok((player, controls))
}

/// Shows `snap`; the track only when it is not `shown` already.
fn show(controls: &SystemMediaTransportControls, snap: &Snapshot, shown: &mut Option<Option<Track>>) -> Result<()> {
    controls.SetPlaybackStatus(playback(snap.status))?;
    if shown.as_ref() == Some(&snap.track) {
        return Ok(());
    }
    let display = controls.DisplayUpdater()?;
    match &snap.track {
        None => display.ClearAll()?,
        Some(t) => describe(&display, t)?,
    }
    display.Update()?;
    *shown = Some(snap.track.clone());
    Ok(())
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
}
