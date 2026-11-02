//! The playback engine thread.
//!
//! One thread decodes, runs the DSP chain and writes to ALSA with blocking
//! writes; ALSA's own buffer is the output buffer. Positions are derived from
//! frames written minus the device delay, so the UI follows what is audible.

use crate::alsa::AlsaPcm;
use crate::decoder::{AudioSource, Decoder, Status, StreamInfo};
use crate::dsp::{balance_gains, db_to_gain, volume_gain, EqSettings, Equalizer};
use crate::stream::{self, StreamHeaders, TitleSlot};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use popamp_core::meta::TrackMeta;
use popamp_core::playlist::is_url;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayGainSettings {
    pub enabled: bool,
    pub album: bool,
    pub preamp_db: f32,
    pub non_rg_db: f32,
}

impl Default for ReplayGainSettings {
    fn default() -> Self {
        Self { enabled: false, album: false, preamp_db: 0.0, non_rg_db: -6.0 }
    }
}

#[derive(Debug)]
pub enum EngineCmd {
    /// Start playing `location`. `id` identifies the playlist entry.
    Open {
        id: u64,
        location: String,
        start_ms: u64,
    },
    /// The track to continue with gaplessly when the current one ends.
    SetNext(Option<(u64, String)>),
    Pause(bool),
    Stop,
    StopFade,
    Seek(u64),
    Volume(u8),
    Balance(i8),
    Eq(EqSettings),
    ReplayGain(ReplayGainSettings),
    Config {
        buffer_ms: u32,
        fade_ms: u32,
        proxy: String,
    },
    Quit,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Started {
        id: u64,
        info: StreamInfo,
        meta: TrackMeta,
        stream: Option<StreamHeaders>,
    },
    /// Gapless switch reached the speakers.
    TrackChanged {
        id: u64,
        info: StreamInfo,
        meta: TrackMeta,
    },
    /// Playback ran out (no next track was queued).
    Finished {
        id: u64,
    },
    Stopped,
    Error {
        id: u64,
        message: String,
    },
    StreamTitle {
        id: u64,
        title: String,
    },
}

const VIS_LEN: usize = 1 << 15;

/// Mono samples of what was written, indexed by the global frame counter.
pub struct VisRing {
    buf: Vec<f32>,
    written: u64,
}

impl VisRing {
    fn new() -> Self {
        Self { buf: vec![0.0; VIS_LEN], written: 0 }
    }

    fn push_stereo(&mut self, samples: &[f32], start_frame: u64) {
        self.written = start_frame;
        for f in samples.as_chunks::<2>().0 {
            self.buf[(self.written as usize) & (VIS_LEN - 1)] = (f[0] + f[1]) * 0.5;
            self.written += 1;
        }
    }

    /// Copies the `out.len()` samples that end at frame `upto`.
    pub fn read(&self, upto: u64, out: &mut [f32]) {
        let upto = upto.min(self.written);
        let n = out.len() as u64;
        for (i, o) in out.iter_mut().enumerate() {
            let frame = (upto + i as u64).checked_sub(n);
            *o = match frame {
                Some(f) if self.written - f < VIS_LEN as u64 => self.buf[(f as usize) & (VIS_LEN - 1)],
                _ => 0.0,
            };
        }
    }
}

/// State the UI reads without talking to the engine thread.
pub struct Shared {
    state: AtomicU8,
    position_ms: AtomicU64,
    /// `u64::MAX` = unknown.
    length_ms: AtomicU64,
    bitrate_kbps: AtomicU32,
    sample_rate: AtomicU32,
    channels: AtomicU32,
    played_frames: AtomicU64,
    pub vis: Mutex<VisRing>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            state: AtomicU8::new(0),
            position_ms: AtomicU64::new(0),
            length_ms: AtomicU64::new(u64::MAX),
            bitrate_kbps: AtomicU32::new(0),
            sample_rate: AtomicU32::new(0),
            channels: AtomicU32::new(0),
            played_frames: AtomicU64::new(0),
            vis: Mutex::new(VisRing::new()),
        }
    }

    pub fn state(&self) -> PlayState {
        match self.state.load(Ordering::Relaxed) {
            1 => PlayState::Playing,
            2 => PlayState::Paused,
            _ => PlayState::Stopped,
        }
    }
    pub fn position_ms(&self) -> u64 {
        self.position_ms.load(Ordering::Relaxed)
    }
    pub fn length_ms(&self) -> Option<u64> {
        Some(self.length_ms.load(Ordering::Relaxed)).filter(|&l| l != u64::MAX)
    }
    pub fn bitrate_kbps(&self) -> u32 {
        self.bitrate_kbps.load(Ordering::Relaxed)
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }
    pub fn channels(&self) -> u32 {
        self.channels.load(Ordering::Relaxed)
    }
    /// Global count of audible frames (for vis alignment).
    pub fn played_frames(&self) -> u64 {
        self.played_frames.load(Ordering::Relaxed)
    }
}

pub struct Engine {
    tx: Sender<EngineCmd>,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Engine {
    pub fn spawn(device: String, on_event: impl Fn(EngineEvent) + Send + 'static) -> Engine {
        let (tx, rx) = crossbeam_channel::unbounded();
        let shared = Arc::new(Shared::new());
        let worker_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("popamp-audio".into())
            .spawn(move || Worker::new(device, rx, worker_shared, Box::new(on_event)).run())
            .expect("spawn audio thread");
        Engine { tx, shared, thread: Some(thread) }
    }

    /// An engine without an audio thread or device, for rendering the
    /// windows offline. Commands are dropped; `simulate` sets what the
    /// windows show.
    pub fn headless() -> Engine {
        let (tx, _) = crossbeam_channel::unbounded();
        Engine { tx, shared: Arc::new(Shared::new()), thread: None }
    }

    /// Publishes a playback state as if a track were playing. Only a
    /// headless engine allows this: a real worker owns these values.
    pub fn simulate(&self, state: PlayState, position_ms: u64, info: &StreamInfo) {
        assert!(self.thread.is_none(), "simulate() needs a headless engine");
        let sh = &self.shared;
        sh.state.store(state as u8, Ordering::Relaxed);
        sh.position_ms.store(position_ms, Ordering::Relaxed);
        sh.length_ms.store(info.length_ms.unwrap_or(u64::MAX), Ordering::Relaxed);
        sh.bitrate_kbps.store(info.bitrate_kbps.unwrap_or(0), Ordering::Relaxed);
        sh.sample_rate.store(info.sample_rate, Ordering::Relaxed);
        sh.channels.store(info.channels as u32, Ordering::Relaxed);
    }

    pub fn send(&self, cmd: EngineCmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(EngineCmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A source plus its tags, ICY title slot, stream headers and start offset.
type Opened = (Box<dyn AudioSource>, TrackMeta, Option<TitleSlot>, Option<StreamHeaders>, u64);

struct Track {
    id: u64,
    dec: Box<dyn AudioSource>,
    gain: f32,
    title: Option<TitleSlot>,
    last_title: Option<String>,
}

struct PendingSwitch {
    at_frame: u64,
    id: u64,
    info: StreamInfo,
    meta: TrackMeta,
}

struct Worker {
    device: String,
    rx: Receiver<EngineCmd>,
    shared: Arc<Shared>,
    emit: Box<dyn Fn(EngineEvent) + Send>,
    pcm: Option<AlsaPcm>,
    track: Option<Track>,
    next: Option<(u64, String)>,
    state: PlayState,
    volume: u8,
    balance: i8,
    eq: Equalizer,
    rg: ReplayGainSettings,
    buffer_ms: u32,
    fade_ms: u32,
    proxy: String,
    /// Remaining/total frames of a stop-fade.
    fade: Option<(u64, u64)>,
    /// Global frames written to the device.
    written: u64,
    /// Global frame at which the current track's `seek_base_ms` plays.
    track_start: u64,
    seek_base_ms: u64,
    /// Paused by dropping the buffer (device can't pause): reseek on resume.
    resume_seek: Option<u64>,
    pending: Option<PendingSwitch>,
    buf: Vec<f32>,
}

impl Worker {
    fn new(device: String, rx: Receiver<EngineCmd>, shared: Arc<Shared>, emit: Box<dyn Fn(EngineEvent) + Send>) -> Self {
        Self {
            device,
            rx,
            shared,
            emit,
            pcm: None,
            track: None,
            next: None,
            state: PlayState::Stopped,
            volume: 200,
            balance: 0,
            eq: Equalizer::new(44_100),
            rg: ReplayGainSettings::default(),
            buffer_ms: 150,
            fade_ms: 1000,
            proxy: String::new(),
            fade: None,
            written: 0,
            track_start: 0,
            seek_base_ms: 0,
            resume_seek: None,
            pending: None,
            buf: Vec::with_capacity(8192),
        }
    }

    fn run(mut self) {
        loop {
            let cmd = if self.state == PlayState::Playing {
                match self.rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(crossbeam_channel::TryRecvError::Empty) => None,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => return,
                }
            } else {
                match self.rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            if let Some(cmd) = cmd {
                if matches!(cmd, EngineCmd::Quit) {
                    return;
                }
                self.handle(cmd);
                continue; // drain all queued commands before decoding more
            }
            if self.state == PlayState::Playing {
                self.step();
            }
            self.publish();
        }
    }

    fn played_frames(&self) -> u64 {
        let delay = self.pcm.as_ref().map(|p| p.delay()).unwrap_or(0);
        self.written.saturating_sub(delay)
    }

    fn rate(&self) -> u64 {
        self.pcm.as_ref().map(|p| p.rate as u64).unwrap_or(44_100).max(1)
    }

    fn position_ms(&self) -> u64 {
        let played = self.played_frames().saturating_sub(self.track_start);
        self.seek_base_ms + played * 1000 / self.rate()
    }

    fn set_state(&mut self, s: PlayState) {
        self.state = s;
        self.shared.state.store(s as u8, Ordering::Relaxed);
    }

    fn publish(&mut self) {
        let played = self.played_frames();
        self.shared.played_frames.store(played, Ordering::Relaxed);
        if let Some(p) = &self.pending {
            if played >= p.at_frame {
                let p = self.pending.take().expect("checked above");
                self.shared.length_ms.store(p.info.length_ms.unwrap_or(u64::MAX), Ordering::Relaxed);
                self.shared.channels.store(p.info.channels as u32, Ordering::Relaxed);
                (self.emit)(EngineEvent::TrackChanged { id: p.id, info: p.info, meta: p.meta });
            } else {
                return; // keep showing the old track until the switch is audible
            }
        }
        let pos = if self.state == PlayState::Stopped { 0 } else { self.position_ms() };
        self.shared.position_ms.store(pos, Ordering::Relaxed);
        if let Some(t) = &mut self.track {
            let info = t.dec.info();
            self.shared.length_ms.store(info.length_ms.unwrap_or(u64::MAX), Ordering::Relaxed);
            self.shared.bitrate_kbps.store(info.bitrate_kbps.unwrap_or(0), Ordering::Relaxed);
            self.shared.sample_rate.store(info.sample_rate, Ordering::Relaxed);
            if let Some(slot) = &t.title {
                let current = slot.lock().clone();
                if current.is_some() && current != t.last_title {
                    t.last_title = current.clone();
                    (self.emit)(EngineEvent::StreamTitle { id: t.id, title: current.unwrap_or_default() });
                }
            }
        }
    }

    fn handle(&mut self, cmd: EngineCmd) {
        match cmd {
            EngineCmd::Open { id, location, start_ms } => self.open(id, &location, start_ms),
            EngineCmd::SetNext(n) => self.next = n,
            EngineCmd::Pause(on) => self.pause(on),
            EngineCmd::Stop => self.stop(true),
            EngineCmd::StopFade => {
                if self.state == PlayState::Playing {
                    let total = self.rate() * self.fade_ms as u64 / 1000;
                    self.fade = Some((total.max(1), total.max(1)));
                } else {
                    self.stop(true);
                }
            }
            EngineCmd::Seek(ms) => self.seek(ms),
            EngineCmd::Volume(v) => self.volume = v,
            EngineCmd::Balance(b) => self.balance = b,
            EngineCmd::Eq(s) => self.eq.set(s),
            EngineCmd::ReplayGain(rg) => {
                self.rg = rg;
                // Takes effect from the next track.
            }
            EngineCmd::Config { buffer_ms, fade_ms, proxy } => {
                self.buffer_ms = buffer_ms;
                self.fade_ms = fade_ms;
                self.proxy = proxy;
            }
            EngineCmd::Quit => {}
        }
    }

    fn track_gain(&self, meta: &TrackMeta) -> f32 {
        if !self.rg.enabled {
            return 1.0;
        }
        let (gain, peak) = if self.rg.album {
            (meta.replaygain_album_gain.or(meta.replaygain_track_gain), meta.replaygain_album_peak.or(meta.replaygain_track_peak))
        } else {
            (meta.replaygain_track_gain.or(meta.replaygain_album_gain), meta.replaygain_track_peak.or(meta.replaygain_album_peak))
        };
        match gain {
            Some(g) => {
                let lin = db_to_gain(g + self.rg.preamp_db);
                // Prevent clipping when the peak is known.
                match peak {
                    Some(p) if p > 0.0 => lin.min(1.0 / p),
                    _ => lin,
                }
            }
            None => db_to_gain(self.rg.non_rg_db),
        }
    }

    fn open_decoder(&self, location: &str, start_ms: u64) -> Result<Opened, String> {
        if crate::youtube::is_youtube(location) {
            // ffmpeg seeks on its own; start where asked.
            let (src, meta) = crate::youtube::PipeSource::open(location, start_ms)?;
            Ok((Box::new(src), meta, None, None, start_ms))
        } else if is_url(location) {
            let (src, ext, headers, title) = stream::open(location, &self.proxy)?;
            let (dec, mut meta) = Decoder::open_source(src, ext, false)?;
            if meta.title.is_none() {
                meta.title = headers.name.clone();
            }
            Ok((Box::new(dec), meta, Some(title), Some(headers), 0))
        } else {
            let (dec, meta) = Decoder::open_file(Path::new(location))?;
            Ok((Box::new(dec), meta, None, None, 0))
        }
    }

    /// Drops buffered audio and rebases the counters on what was audible.
    fn discard_output(&mut self) {
        let played = self.played_frames();
        if let Some(p) = &mut self.pcm {
            p.discard();
        }
        self.written = played;
        self.pending = None;
    }

    fn ensure_pcm(&mut self, rate: u32) -> Result<(), String> {
        if self.pcm.as_ref().is_some_and(|p| p.rate == rate) {
            return Ok(());
        }
        if let Some(mut old) = self.pcm.take() {
            old.drain();
            self.written = 0; // new device clock
        }
        self.pcm = Some(AlsaPcm::open(&self.device, rate, 2, self.buffer_ms)?);
        self.written = 0;
        self.eq.set_rate(rate);
        Ok(())
    }

    fn open(&mut self, id: u64, location: &str, start_ms: u64) {
        self.discard_output();
        self.track = None;
        self.fade = None;
        self.resume_seek = None;
        self.next = None;
        match self.open_decoder(location, start_ms) {
            Ok((mut dec, meta, title, headers, started_at)) => {
                if let Err(e) = self.ensure_pcm(dec.info().sample_rate) {
                    self.fail(id, e);
                    return;
                }
                let mut base = started_at;
                if start_ms > 0 && started_at == 0 && dec.info().seekable {
                    base = dec.seek(start_ms).unwrap_or(0);
                }
                self.track_start = self.written;
                self.seek_base_ms = base;
                self.eq.reset();
                let gain = self.track_gain(&meta);
                let info = dec.info().clone();
                self.shared.channels.store(info.channels as u32, Ordering::Relaxed);
                self.track = Some(Track { id, dec, gain, title, last_title: None });
                self.set_state(PlayState::Playing);
                (self.emit)(EngineEvent::Started { id, info, meta, stream: headers });
            }
            Err(e) => self.fail(id, e),
        }
    }

    fn fail(&mut self, id: u64, message: String) {
        log::warn!("playback error: {message}");
        self.track = None;
        self.set_state(PlayState::Stopped);
        self.shared.position_ms.store(0, Ordering::Relaxed);
        (self.emit)(EngineEvent::Error { id, message });
    }

    fn pause(&mut self, on: bool) {
        match (on, self.state) {
            (true, PlayState::Playing) => {
                let pos = self.position_ms();
                let hw = self.pcm.as_mut().is_some_and(|p| p.pause(true));
                if !hw {
                    self.discard_output();
                    self.resume_seek = Some(pos);
                }
                self.set_state(PlayState::Paused);
            }
            (false, PlayState::Paused) => {
                if let Some(pos) = self.resume_seek.take() {
                    self.seek_to(pos);
                } else if let Some(p) = &mut self.pcm {
                    if !p.pause(false) {
                        p.discard();
                    }
                }
                self.set_state(PlayState::Playing);
            }
            _ => {}
        }
    }

    fn seek_to(&mut self, ms: u64) {
        let Some(t) = &mut self.track else { return };
        if !t.dec.info().seekable {
            return;
        }
        let reached = t.dec.seek(ms).unwrap_or(ms);
        self.discard_output();
        self.track_start = self.written;
        self.seek_base_ms = reached;
        self.eq.reset();
        // Publish now so a quick second relative seek starts from here.
        self.shared.position_ms.store(reached, Ordering::Relaxed);
    }

    fn seek(&mut self, ms: u64) {
        if self.state == PlayState::Paused {
            // Resume from the new position later.
            if self.resume_seek.is_some() {
                self.resume_seek = Some(ms);
                return;
            }
            if let Some(p) = &mut self.pcm {
                p.pause(false);
            }
            self.seek_to(ms);
            self.resume_seek = Some(ms);
            return;
        }
        self.seek_to(ms);
    }

    fn stop(&mut self, notify: bool) {
        if let Some(p) = &mut self.pcm {
            p.discard();
        }
        self.written = self.played_frames();
        self.track = None;
        self.next = None;
        self.fade = None;
        self.pending = None;
        self.resume_seek = None;
        self.set_state(PlayState::Stopped);
        self.shared.position_ms.store(0, Ordering::Relaxed);
        self.shared.bitrate_kbps.store(0, Ordering::Relaxed);
        self.shared.length_ms.store(u64::MAX, Ordering::Relaxed);
        if notify {
            (self.emit)(EngineEvent::Stopped);
        }
    }

    fn step(&mut self) {
        let Some(track) = &mut self.track else {
            self.set_state(PlayState::Stopped);
            return;
        };
        self.buf.clear();
        match track.dec.next(&mut self.buf) {
            Ok(Status::Data) => {}
            Ok(Status::Eof) => return self.end_of_track(),
            Err(e) => {
                let id = track.id;
                log::warn!("{e}");
                // A broken tail is treated like the end of the file.
                if self.buf.is_empty() {
                    (self.emit)(EngineEvent::Error { id, message: e });
                    return self.end_of_track();
                }
            }
        }
        let gain = track.gain;
        let mut buf = std::mem::take(&mut self.buf);
        if gain != 1.0 {
            buf.iter_mut().for_each(|s| *s *= gain);
        }
        self.eq.process(&mut buf);
        // Vis sees the signal before volume, like the original.
        self.shared.vis.lock().push_stereo(&buf, self.written);
        let vol = volume_gain(self.volume);
        let (gl, gr) = balance_gains(self.balance);
        let mut fade_done = false;
        for frame in buf.as_chunks_mut::<2>().0 {
            let mut g = vol;
            if let Some((left, total)) = &mut self.fade {
                g *= *left as f32 / *total as f32;
                *left = left.saturating_sub(1);
                fade_done |= *left == 0;
            }
            frame[0] = (frame[0] * g * gl).clamp(-1.0, 1.0);
            frame[1] = (frame[1] * g * gr).clamp(-1.0, 1.0);
        }
        let result = match &mut self.pcm {
            Some(p) => p.write(&buf),
            None => Err("no output device".into()),
        };
        self.written += (buf.len() / 2) as u64;
        self.buf = buf;
        if let Err(e) = result {
            let id = self.track.as_ref().map(|t| t.id).unwrap_or(0);
            self.pcm = None;
            self.fail(id, e);
            return;
        }
        if fade_done {
            if let Some(p) = &mut self.pcm {
                p.drain();
            }
            self.stop(true);
        }
    }

    fn end_of_track(&mut self) {
        let finished_id = self.track.as_ref().map(|t| t.id).unwrap_or(0);
        if let Some((id, location)) = self.next.take() {
            match self.open_decoder(&location, 0) {
                Ok((dec, meta, title, _, _)) => {
                    let same_rate = self.pcm.as_ref().is_some_and(|p| p.rate == dec.info().sample_rate);
                    if !same_rate {
                        if let Err(e) = self.ensure_pcm(dec.info().sample_rate) {
                            return self.fail(id, e);
                        }
                    }
                    let info = dec.info().clone();
                    let gain = self.track_gain(&meta);
                    self.track = Some(Track { id, dec, gain, title, last_title: None });
                    self.track_start = self.written;
                    self.seek_base_ms = 0;
                    self.pending = Some(PendingSwitch { at_frame: self.written, id, info, meta });
                    return;
                }
                Err(e) => {
                    // Let the controller decide what comes after a broken track.
                    if let Some(p) = &mut self.pcm {
                        p.drain();
                    }
                    self.track = None;
                    self.set_state(PlayState::Stopped);
                    (self.emit)(EngineEvent::Error { id, message: e });
                    return;
                }
            }
        }
        if let Some(p) = &mut self.pcm {
            p.drain();
        }
        self.written = self.played_frames();
        self.track = None;
        self.set_state(PlayState::Stopped);
        self.shared.position_ms.store(0, Ordering::Relaxed);
        self.shared.length_ms.store(u64::MAX, Ordering::Relaxed);
        (self.emit)(EngineEvent::Finished { id: finished_id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vis_ring_reads_aligned_window() {
        let mut v = VisRing::new();
        let samples: Vec<f32> = (0..200).flat_map(|i| [i as f32, i as f32]).collect();
        v.push_stereo(&samples, 1000);
        let mut out = [0.0; 4];
        v.read(1100, &mut out);
        assert_eq!(out, [96.0, 97.0, 98.0, 99.0]);
        v.read(5000, &mut out); // clamps to what was written
        assert_eq!(out, [196.0, 197.0, 198.0, 199.0]);
        v.read(1002, &mut out); // before the first frame
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn headless_engine_reports_simulated_playback() {
        let engine = Engine::headless();
        assert_eq!(engine.shared().state(), PlayState::Stopped);
        let info = StreamInfo {
            sample_rate: 44_100,
            channels: 2,
            length_ms: Some(225_000),
            bitrate_kbps: Some(128),
            codec: "mp3".into(),
            seekable: true,
        };
        engine.simulate(PlayState::Playing, 83_000, &info);
        let sh = engine.shared();
        assert_eq!(sh.state(), PlayState::Playing);
        assert_eq!(sh.position_ms(), 83_000);
        assert_eq!(sh.length_ms(), Some(225_000));
        assert_eq!(sh.bitrate_kbps(), 128);
        assert_eq!(sh.sample_rate(), 44_100);
        assert_eq!(sh.channels(), 2);
        // Commands go nowhere instead of panicking.
        engine.send(EngineCmd::Stop);
    }
}
