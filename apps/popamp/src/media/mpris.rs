//! MPRIS2 D-Bus service, the media controls on Unix.
//!
//! A dedicated thread owns the connection. Method calls are forwarded to the
//! UI thread as remote commands; the UI thread sends [`Snapshot`]s and the
//! thread emits `PropertiesChanged` for whatever differs.

use super::Snapshot;
use crate::cli::Remote;
use crate::ipc::Request;
use crate::model::AppEvent;
use parking_lot::Mutex;
use popamp_audio::Shared;
use popamp_core::Command;
use std::collections::HashMap;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";

#[derive(Clone)]
struct Ctx {
    proxy: EventLoopProxy<AppEvent>,
    snap: Arc<Mutex<Snapshot>>,
    shared: Arc<Shared>,
}

impl Ctx {
    fn send(&self, remotes: Vec<Remote>) {
        let (tx, _rx) = crossbeam_channel::bounded(1);
        let _ = self.proxy.send_event(AppEvent::Remote(Request { remotes, reply: tx }));
    }
}

fn owned<'a>(v: impl Into<Value<'a>>) -> Option<OwnedValue> {
    v.into().try_to_owned().ok()
}

struct Root {
    ctx: Ctx,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        self.ctx.send(vec![Remote::Show]);
    }
    fn quit(&self) {
        self.ctx.send(vec![Remote::Quit]);
    }
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        crate::edition::NAME.into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "popamp".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".into(), "http".into(), "https".into()]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        [
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/x-vorbis+ogg",
            "audio/mp4",
            "audio/aac",
            "audio/x-wav",
            "audio/x-aiff",
            "audio/x-mpegurl",
            "audio/x-scpls",
        ]
        .map(String::from)
        .to_vec()
    }
}

struct Player {
    ctx: Ctx,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.ctx.send(vec![Remote::Cmd(Command::Next)]);
    }
    fn previous(&self) {
        self.ctx.send(vec![Remote::Cmd(Command::Previous)]);
    }
    fn pause(&self) {
        if self.ctx.snap.lock().status == "Playing" {
            self.ctx.send(vec![Remote::Cmd(Command::Pause)]);
        }
    }
    fn play_pause(&self) {
        self.ctx.send(vec![Remote::Cmd(Command::PlayPause)]);
    }
    fn stop(&self) {
        self.ctx.send(vec![Remote::Cmd(Command::Stop)]);
    }
    fn play(&self) {
        self.ctx.send(vec![Remote::Cmd(Command::Play)]);
    }
    fn seek(&self, offset_us: i64) {
        let pos = self.ctx.shared.position_ms() as i64 + offset_us / 1000;
        self.ctx.send(vec![Remote::SeekMs(pos.max(0) as u64)]);
    }
    fn set_position(&self, track_id: ObjectPath<'_>, position_us: i64) {
        let current = self.ctx.snap.lock().track.as_ref().map(|t| track_path(t.id));
        if current.as_deref() == Some(track_id.as_str()) && position_us >= 0 {
            self.ctx.send(vec![Remote::SeekMs(position_us as u64 / 1000)]);
        }
    }
    fn open_uri(&self, uri: String) {
        let loc = match uri.strip_prefix("file://") {
            Some(p) => popamp_core::text::percent_decode(p),
            None => uri,
        };
        self.ctx.send(vec![Remote::Open(vec![loc])]);
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.ctx.snap.lock().status.to_string()
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
        if self.ctx.snap.lock().repeat { "Playlist" } else { "None" }.to_string()
    }
    #[zbus(property)]
    fn set_loop_status(&mut self, v: String) {
        self.ctx.send(vec![Remote::Repeat(Some(v != "None"))]);
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_rate(&mut self, _v: f64) {}
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.ctx.snap.lock().shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&mut self, v: bool) {
        self.ctx.send(vec![Remote::Shuffle(Some(v))]);
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.ctx.snap.lock().volume
    }
    #[zbus(property)]
    fn set_volume(&mut self, v: f64) {
        self.ctx.send(vec![Remote::SetVolumePercent((v.clamp(0.0, 1.0) * 100.0).round() as u8)]);
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.ctx.shared.position_ms() as i64 * 1000
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let snap = self.ctx.snap.lock();
        let mut m = HashMap::new();
        let Some(t) = &snap.track else {
            if let Some(v) = ObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack").ok().and_then(owned) {
                m.insert("mpris:trackid".into(), v);
            }
            return m;
        };
        let path = track_path(t.id);
        if let Some(v) = ObjectPath::try_from(path.as_str()).ok().and_then(owned) {
            m.insert("mpris:trackid".into(), v);
        }
        if let Some(v) = owned(t.title.as_str()) {
            m.insert("xesam:title".into(), v);
        }
        if let Some(v) = t.artist.as_ref().and_then(|a| owned(vec![a.as_str()])) {
            m.insert("xesam:artist".into(), v);
        }
        if let Some(v) = t.album.as_deref().and_then(owned) {
            m.insert("xesam:album".into(), v);
        }
        if let Some(v) = t.length_ms.and_then(|l| owned(l as i64 * 1000)) {
            m.insert("mpris:length".into(), v);
        }
        let url = if popamp_core::playlist::is_url(&t.url) { t.url.clone() } else { popamp_core::text::path_to_file_url(&t.url) };
        if let Some(v) = owned(url.as_str()) {
            m.insert("xesam:url".into(), v);
        }
        m
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.ctx.snap.lock().track.as_ref().is_some_and(|t| t.length_ms.is_some())
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

fn track_path(id: u64) -> String {
    format!("/org/popamp/track/{id}")
}

/// Starts the service; `None` if there is no session bus.
pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, instance: &str) -> Option<crossbeam_channel::Sender<Snapshot>> {
    let (tx, rx) = crossbeam_channel::unbounded::<Snapshot>();
    let name = if instance == "default" {
        "org.mpris.MediaPlayer2.popamp".to_string()
    } else {
        format!("org.mpris.MediaPlayer2.popamp.instance{}", std::process::id())
    };
    let snap = Arc::new(Mutex::new(Snapshot::default()));
    let ctx = Ctx { proxy, snap: snap.clone(), shared };
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(name.as_str()))
        .and_then(|b| b.serve_at(PATH, Root { ctx: ctx.clone() }))
        .and_then(|b| b.serve_at(PATH, Player { ctx }))
        .and_then(|b| b.build());
    let conn = match conn {
        Ok(c) => c,
        Err(e) => {
            log::warn!("MPRIS unavailable: {e}");
            return None;
        }
    };
    std::thread::Builder::new()
        .name("popamp-mpris".into())
        .spawn(move || {
            let iface = match conn.object_server().interface::<_, Player>(PATH) {
                Ok(i) => i,
                Err(e) => {
                    log::warn!("MPRIS: {e}");
                    return;
                }
            };
            for new in rx {
                let old = std::mem::replace(&mut *snap.lock(), new.clone());
                let emitter = iface.signal_emitter();
                let player = iface.get();
                let res = zbus::block_on(async {
                    if old.status != new.status {
                        player.playback_status_changed(emitter).await?;
                    }
                    if old.track != new.track {
                        player.metadata_changed(emitter).await?;
                        player.can_seek_changed(emitter).await?;
                    }
                    if old.repeat != new.repeat {
                        player.loop_status_changed(emitter).await?;
                    }
                    if old.shuffle != new.shuffle {
                        player.shuffle_changed(emitter).await?;
                    }
                    if old.volume != new.volume {
                        player.volume_changed(emitter).await?;
                    }
                    zbus::Result::Ok(())
                });
                if let Err(e) = res {
                    log::debug!("MPRIS signal: {e}");
                }
            }
            drop(conn);
        })
        .ok()?;
    Some(tx)
}
