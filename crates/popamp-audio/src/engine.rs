//! The playback engine thread.
//!
//! One thread decodes, runs the DSP chain and writes to ALSA with blocking
//! writes; ALSA's own buffer is the output buffer. Positions are derived from
//! frames written minus the device delay, so the UI follows what is audible.

use crate::decoder::{AudioSource, Decoder, Status, StreamInfo};
use crate::dsp::{balance_gains, db_to_gain, volume_gain, Effect, Effects, EqSettings, Equalizer};
use crate::output::{self, Pcm};
use crate::pipe::PipeSource;
use crate::source::{Context, Playback, Sources};
use crate::stream::{self, StreamHeaders, TitleSlot};
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use popamp_core::meta::TrackMeta;
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
    /// The effects to run after the equalizer, in order.
    Effects(Vec<Box<dyn Effect>>),
    ReplayGain(ReplayGainSettings),
    Config {
        /// The output device, by its name in [`crate::output::devices`].
        device: String,
        buffer_ms: u32,
        fade_ms: u32,
        /// What opening a track needs from the settings.
        cx: Context,
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
    /// Seeks the worker has made.
    seeks: AtomicU64,
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
            seeks: AtomicU64::new(0),
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
    /// Seeks sent, and where the last one goes.
    seeks: AtomicU64,
    seek_to: AtomicU64,
}

impl Engine {
    /// An engine that plays what `sources` say how to play.
    pub fn spawn(device: String, sources: Arc<Sources>, on_event: impl Fn(EngineEvent) + Send + 'static) -> Engine {
        let (tx, rx) = crossbeam_channel::unbounded();
        let shared = Arc::new(Shared::new());
        let worker_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("popamp-audio".into())
            .spawn(move || Worker::new(device, sources, rx, worker_shared, Box::new(on_event)).run())
            .expect("spawn audio thread");
        Engine { tx, shared, thread: Some(thread), seeks: AtomicU64::new(0), seek_to: AtomicU64::new(0) }
    }

    /// An engine without an audio thread or device, for rendering the
    /// windows offline. Commands are dropped; `simulate` sets what the
    /// windows show.
    pub fn headless() -> Engine {
        let (tx, _) = crossbeam_channel::unbounded();
        Engine { tx, shared: Arc::new(Shared::new()), thread: None, seeks: AtomicU64::new(0), seek_to: AtomicU64::new(0) }
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
        if let EngineCmd::Seek(ms) = cmd {
            self.seek_to.store(ms, Ordering::Relaxed);
            self.seeks.fetch_add(1, Ordering::Relaxed);
        }
        let _ = self.tx.send(cmd);
    }

    /// The position to show: where a seek goes until the worker has made
    /// it, so a slider let go does not jump back meanwhile.
    pub fn position_ms(&self) -> u64 {
        if self.shared.seeks.load(Ordering::Acquire) < self.seeks.load(Ordering::Relaxed) {
            self.seek_to.load(Ordering::Relaxed)
        } else {
            self.shared.position_ms()
        }
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

/// What a track is opened for.
#[derive(Clone, Copy)]
enum Start {
    /// Play it now, from `start_ms`.
    Now { start_ms: u64 },
    /// Continue with it when the current track ends, without a gap.
    Gapless,
}

/// A track opened on a thread of its own. `ticket` tells which request
/// it answers; the worker drops answers to requests it no longer wants.
struct OpenDone {
    ticket: u64,
    id: u64,
    start: Start,
    result: Result<Opened, String>,
}

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
    pcm: Option<Pcm>,
    track: Option<Track>,
    next: Option<(u64, String)>,
    state: PlayState,
    volume: u8,
    balance: i8,
    eq: Equalizer,
    effects: Effects,
    rg: ReplayGainSettings,
    buffer_ms: u32,
    fade_ms: u32,
    sources: Arc<Sources>,
    cx: Context,
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
    /// Opening a track can take seconds (a stream connecting), so it runs
    /// on its own thread and answers here; commands are handled meanwhile.
    opened_tx: Sender<OpenDone>,
    opened_rx: Receiver<OpenDone>,
    /// The last ticket given out, and the one the worker is waiting for.
    tickets: u64,
    opening: Option<u64>,
}

impl Worker {
    fn new(
        device: String,
        sources: Arc<Sources>,
        rx: Receiver<EngineCmd>,
        shared: Arc<Shared>,
        emit: Box<dyn Fn(EngineEvent) + Send>,
    ) -> Self {
        let (opened_tx, opened_rx) = crossbeam_channel::unbounded();
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
            effects: Effects::new(44_100),
            rg: ReplayGainSettings::default(),
            buffer_ms: 150,
            fade_ms: 1000,
            sources,
            cx: Context::default(),
            fade: None,
            written: 0,
            track_start: 0,
            seek_base_ms: 0,
            resume_seek: None,
            pending: None,
            buf: Vec::with_capacity(8192),
            opened_tx,
            opened_rx,
            tickets: 0,
            opening: None,
        }
    }

    fn run(mut self) {
        let (commands, opened) = (self.rx.clone(), self.opened_rx.clone());
        loop {
            let decoding = self.state == PlayState::Playing && self.track.is_some();
            let wait = if decoding { Duration::ZERO } else { Duration::from_millis(100) };
            crossbeam_channel::select! {
                recv(commands) -> cmd => match cmd {
                    Ok(EngineCmd::Quit) | Err(_) => return,
                    Ok(cmd) => {
                        self.handle(cmd);
                        continue; // drain all queued commands before decoding more
                    }
                },
                recv(opened) -> done => {
                    if let Ok(done) = done {
                        self.opened(done);
                    }
                    continue;
                },
                default(wait) => {}
            }
            if decoding {
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
            EngineCmd::Open { id, location, start_ms } => self.open(id, location, start_ms),
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
            EngineCmd::Seek(ms) => {
                self.seek(ms);
                self.publish();
                self.shared.seeks.fetch_add(1, Ordering::Release);
            }
            EngineCmd::Volume(v) => self.volume = v,
            EngineCmd::Balance(b) => self.balance = b,
            EngineCmd::Eq(s) => self.eq.set(s),
            EngineCmd::Effects(list) => self.effects.set(list),
            EngineCmd::ReplayGain(rg) => {
                self.rg = rg;
                // Takes effect from the next track.
            }
            EngineCmd::Config { device, buffer_ms, fade_ms, cx } => {
                self.buffer_ms = buffer_ms;
                self.fade_ms = fade_ms;
                self.cx = cx;
                self.set_device(device);
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

    /// Opens `location` on a new thread; the answer comes to `opened`.
    fn request_open(&mut self, id: u64, location: String, start: Start) {
        self.tickets += 1;
        let ticket = self.tickets;
        self.opening = Some(ticket);
        let (tx, sources, cx) = (self.opened_tx.clone(), self.sources.clone(), self.cx.clone());
        let start_ms = match start {
            Start::Now { start_ms } => start_ms,
            Start::Gapless => 0,
        };
        let spawned = std::thread::Builder::new().name("popamp-open".into()).spawn(move || {
            let result = open_location(&sources, &location, start_ms, &cx);
            let _ = tx.send(OpenDone { ticket, id, start, result });
        });
        if let Err(e) = spawned {
            self.opening = None;
            self.fail(id, format!("cannot open a track: {e}"));
        }
    }

    fn opened(&mut self, done: OpenDone) {
        if self.opening != Some(done.ticket) {
            return; // stopped or replaced meanwhile; dropping it closes it
        }
        self.opening = None;
        match done.start {
            Start::Now { start_ms } => self.start_now(done.id, start_ms, done.result),
            Start::Gapless => self.continue_with(done.id, done.result),
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
        self.pcm = Some(match Pcm::open(&self.device, rate, 2, self.buffer_ms) {
            Ok(pcm) => pcm,
            // An unplugged device leaves the default one, not silence.
            Err(e) if self.device != output::DEFAULT => {
                log::warn!("{e}; playing on the default device");
                Pcm::open(output::DEFAULT, rate, 2, self.buffer_ms)?
            }
            Err(e) => return Err(e),
        });
        self.written = 0;
        self.eq.set_rate(rate);
        self.effects.set_rate(rate);
        Ok(())
    }

    /// Moves playback to another device, from where it was audible.
    fn set_device(&mut self, device: String) {
        if device == self.device {
            return;
        }
        self.device = device;
        let Some(rate) = self.pcm.as_ref().map(|p| p.rate) else { return };
        let at = self.position_ms();
        let pending = self.pending.take();
        self.pcm = None;
        if let Err(e) = self.ensure_pcm(rate) {
            let id = self.track.as_ref().map_or(0, |t| t.id);
            return self.fail(id, e);
        }
        if self.track.is_none() {
            return;
        }
        self.track_start = self.written;
        self.seek_base_ms = at;
        // What the old device had queued is gone: play it again here
        // (streams, which cannot seek, go on).
        if self.state == PlayState::Paused {
            self.resume_seek = Some(at);
        } else {
            self.seek_to(at);
        }
        // A gapless switch already decoding starts on the new clock.
        self.pending = pending.map(|p| PendingSwitch { at_frame: 0, ..p });
    }

    fn open(&mut self, id: u64, location: String, start_ms: u64) {
        self.discard_output();
        self.track = None;
        self.fade = None;
        self.resume_seek = None;
        self.next = None;
        // Playing while it connects: Next, Prev and Stop act on it.
        self.track_start = self.written;
        self.seek_base_ms = start_ms;
        self.shared.length_ms.store(u64::MAX, Ordering::Relaxed);
        self.shared.bitrate_kbps.store(0, Ordering::Relaxed);
        self.set_state(PlayState::Playing);
        self.request_open(id, location, Start::Now { start_ms });
    }

    fn start_now(&mut self, id: u64, start_ms: u64, opened: Result<Opened, String>) {
        match opened {
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
                self.effects.reset();
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

    fn continue_with(&mut self, id: u64, opened: Result<Opened, String>) {
        match opened {
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
            }
            Err(e) => {
                // Let the controller decide what comes after a broken track.
                if let Some(p) = &mut self.pcm {
                    p.drain();
                }
                self.track = None;
                self.set_state(PlayState::Stopped);
                (self.emit)(EngineEvent::Error { id, message: e });
            }
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
        if self.track.is_none() {
            return; // nothing to pause while the next track is opening
        }
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
        self.effects.reset();
        // Publish now so a quick second relative seek starts from here.
        self.shared.position_ms.store(reached, Ordering::Relaxed);
    }

    fn seek(&mut self, ms: u64) {
        if self.state == PlayState::Paused {
            // Resume from the new position later.
            if self.resume_seek.is_some() {
                self.resume_seek = Some(ms);
                self.track_start = self.played_frames();
                self.seek_base_ms = ms;
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
        self.opening = None;
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
            if self.opening.is_none() {
                self.set_state(PlayState::Stopped);
            }
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
        self.effects.process(&mut buf);
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
            // The device still plays its buffer while the next track opens.
            self.track = None;
            return self.request_open(id, location, Start::Gapless);
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

/// Opens a file or a stream; slow for streams, so never on the worker.
fn open_location(sources: &Sources, location: &str, start_ms: u64, cx: &Context) -> Result<Opened, String> {
    match sources.of(location).open(location, cx)? {
        Playback::Pipe(pipe) => {
            // The program seeks on its own; start where asked.
            let (src, meta) = PipeSource::open(*pipe, start_ms)?;
            Ok((Box::new(src), meta, None, None, start_ms))
        }
        Playback::Http(url) => {
            let (src, ext, headers, title) = stream::open(&url, &cx.proxy)?;
            let (dec, mut meta) = Decoder::open_source(src, ext, false)?;
            if meta.title.is_none() {
                meta.title = headers.name.clone();
            }
            Ok((Box::new(dec), meta, Some(title), Some(headers), 0))
        }
        Playback::File(path) => {
            let (dec, meta) = crate::formats::open(&path, cx)?;
            Ok((dec, meta, None, None, 0))
        }
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

    /// An engine whose events arrive on a channel, and a server that
    /// accepts connections and never answers, so a stream hangs connecting.
    fn engine_and_silent_server() -> (Engine, Receiver<EngineEvent>, std::net::TcpListener) {
        let (engine, rx) = engine_with(Sources::default());
        (engine, rx, std::net::TcpListener::bind("127.0.0.1:0").unwrap())
    }

    fn engine_with(sources: Sources) -> (Engine, Receiver<EngineEvent>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = Engine::spawn("null".into(), Arc::new(sources), move |e| {
            let _ = tx.send(e);
        });
        (engine, rx)
    }

    /// The events up to the first `Finished`, in short form.
    fn events_until_finished(events: &Receiver<EngineEvent>) -> Vec<String> {
        let mut seen = Vec::new();
        while let Ok(e) = events.recv_timeout(Duration::from_secs(3)) {
            seen.push(match e {
                EngineEvent::Started { id, .. } => format!("started {id}"),
                EngineEvent::TrackChanged { id, .. } => format!("changed {id}"),
                EngineEvent::Finished { id } => format!("finished {id}"),
                other => format!("{other:?}"),
            });
            if seen.last().is_some_and(|s| s.starts_with("finished")) {
                break;
            }
        }
        seen
    }

    fn silent_url(server: &std::net::TcpListener) -> String {
        format!("http://{}/stream", server.local_addr().unwrap())
    }

    #[test]
    fn stop_works_while_a_stream_is_connecting() {
        let (engine, events, server) = engine_and_silent_server();
        engine.send(EngineCmd::Open { id: 1, location: silent_url(&server), start_ms: 0 });
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(engine.shared().state(), PlayState::Playing, "connecting counts as playing");
        engine.send(EngineCmd::Stop);
        let first = events.recv_timeout(Duration::from_secs(2));
        assert!(matches!(first, Ok(EngineEvent::Stopped)), "got {first:?}");
        drop(server); // the connection fails now; nothing may come of it
        assert!(events.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[test]
    fn a_new_track_replaces_one_still_connecting() {
        let (engine, events, server) = engine_and_silent_server();
        engine.send(EngineCmd::Open { id: 1, location: silent_url(&server), start_ms: 0 });
        std::thread::sleep(Duration::from_millis(100));
        engine.send(EngineCmd::Open { id: 2, location: "/nonexistent/track.mp3".into(), start_ms: 0 });
        let first = events.recv_timeout(Duration::from_secs(2));
        assert!(matches!(first, Ok(EngineEvent::Error { id: 2, .. })), "got {first:?}");
        drop(server);
        assert!(events.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[test]
    fn a_web_page_is_a_link_for_a_plugin_not_a_stream() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/watch?v=a", server.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut c, _) = server.accept().unwrap();
            let _ = c.read(&mut [0; 1024]);
            let page = "<html><body>a video</body></html>";
            let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{page}", page.len());
        });
        let (engine, events) = engine_with(Sources::default());
        engine.send(EngineCmd::Open { id: 1, location: url, start_ms: 0 });
        let first = events.recv_timeout(Duration::from_secs(3));
        assert!(matches!(&first, Ok(EngineEvent::Error { id: 1, message }) if message == "no plug-in plays this link"), "got {first:?}");
    }

    /// A WAV file of `ms` milliseconds of silence, 44.1 kHz 16-bit stereo.
    fn silent_wav(name: &str, ms: u32) -> std::path::PathBuf {
        let data_len = 44_100 * 4 * ms / 1000;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 0x0002_0001, 44_100, 44_100 * 4, 0x0010_0004] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(wav.len() + data_len as usize, 0);
        let path = std::env::temp_dir().join(format!("popamp-engine-{}-{name}.wav", std::process::id()));
        std::fs::write(&path, wav).unwrap();
        path
    }

    #[test]
    fn plays_a_track_and_continues_with_the_next_one() {
        let (engine, events, _server) = engine_and_silent_server();
        let (a, b) = (silent_wav("a", 300), silent_wav("b", 300));
        engine.send(EngineCmd::Open { id: 1, location: a.to_string_lossy().into(), start_ms: 0 });
        engine.send(EngineCmd::SetNext(Some((2, b.to_string_lossy().into()))));
        let seen = events_until_finished(&events);
        let _ = (std::fs::remove_file(a), std::fs::remove_file(b));
        assert_eq!(seen, ["started 1", "changed 2", "finished 2"]);
    }

    /// Writes down what the engine asks of it.
    struct Recorder(Arc<parking_lot::Mutex<Vec<String>>>);

    impl crate::dsp::Effect for Recorder {
        fn reset(&mut self, rate: u32) {
            self.0.lock().push(format!("reset {rate}"));
        }
        fn process(&mut self, block: &mut [f32]) {
            self.0.lock().push(format!("process {}", block.len() % 2));
        }
    }

    #[test]
    fn an_effect_is_reset_at_the_track_rate_and_gets_its_blocks() {
        let (engine, events, _server) = engine_and_silent_server();
        let seen = Arc::new(parking_lot::Mutex::new(Vec::new()));
        engine.send(EngineCmd::Effects(vec![Box::new(Recorder(seen.clone()))]));
        let wav = silent_wav("fx", 300);
        engine.send(EngineCmd::Open { id: 1, location: wav.to_string_lossy().into(), start_ms: 0 });
        events_until_finished(&events);
        let _ = std::fs::remove_file(wav);
        let seen = seen.lock().clone();
        assert_eq!(seen.first().map(String::as_str), Some("reset 44100"), "{seen:?}");
        let resets_then_blocks = seen.iter().skip_while(|s| *s == "reset 44100").all(|s| s == "process 0");
        assert!(resets_then_blocks && seen.last().is_some_and(|s| s == "process 0"), "{seen:?}");
    }

    /// Takes `pipe:` locations and plays them through a shell script;
    /// `live:` ones play the same way but cannot seek.
    #[cfg(unix)]
    struct ShellSource;

    #[cfg(unix)]
    impl crate::source::Source for ShellSource {
        fn id(&self) -> &str {
            "shell"
        }
        fn handles(&self, location: &str) -> bool {
            location.starts_with("pipe:") || location.starts_with("live:")
        }
        fn expand(&self, _: &str, _: &Context) -> Result<Option<Vec<popamp_core::plformats::Item>>, String> {
            Ok(None)
        }
        fn describe(&self, _: &str) -> Option<Result<TrackMeta, String>> {
            None
        }
        fn open(&self, location: &str, _: &Context) -> Result<Playback, String> {
            let pipe = match location.split_once(':') {
                Some(("live", script)) => crate::pipe::Pipe { seekable: false, ..crate::pipe::tests::sh(script) },
                _ => crate::pipe::tests::sh(location.trim_start_matches("pipe:")),
            };
            Ok(Playback::Pipe(Box::new(pipe)))
        }
    }

    // The program is a shell script.
    #[cfg(unix)]
    #[test]
    fn plays_what_an_added_source_says_to_play() {
        let sources = Sources::default();
        sources.set_plugins(vec![Arc::new(ShellSource)]);
        let (engine, events) = engine_with(sources);
        // 0.2 s of stereo f32 silence at 48 kHz.
        engine.send(EngineCmd::Open { id: 1, location: "pipe:head -c 76800 /dev/zero".into(), start_ms: 0 });
        assert_eq!(events_until_finished(&events), ["started 1", "finished 1"]);
    }

    /// A worker driven by hand, playing `location` on ALSA's `null`.
    #[cfg(unix)]
    fn worker_playing(location: &str) -> Worker {
        let sources = Sources::default();
        sources.set_plugins(vec![Arc::new(ShellSource)]);
        let (_, rx) = crossbeam_channel::unbounded();
        let mut w = Worker::new("null".into(), Arc::new(sources), rx, Arc::new(Shared::new()), Box::new(|_| {}));
        let opened = open_location(&w.sources, location, 0, &w.cx);
        w.start_now(1, 0, opened);
        for _ in 0..5 {
            w.step();
        }
        w
    }

    // ALSA's `plug:null` is a second device that plays nothing.
    #[cfg(unix)]
    #[test]
    fn a_new_device_goes_on_from_where_the_track_was() {
        let wav = silent_wav("device", 1000);
        let mut w = worker_playing(&wav.to_string_lossy());
        let before = w.position_ms();
        w.set_device("plug:null".into());
        let after = w.position_ms();
        for _ in 0..5 {
            w.step();
        }
        let _ = std::fs::remove_file(wav);
        assert!(before > 0 && after == before, "{before} ms, then {after} ms");
        assert!(w.state == PlayState::Playing && w.position_ms() > after, "{:?} at {} ms", w.state, w.position_ms());
    }

    #[cfg(unix)]
    #[test]
    fn a_stream_goes_on_counting_on_the_new_device() {
        let mut w = worker_playing("live:head -c 384000 /dev/zero");
        assert!(!w.track.as_ref().unwrap().dec.info().seekable);
        let before = w.position_ms();
        w.set_device("plug:null".into());
        assert!(before > 0 && w.position_ms() == before, "{before} ms, then {} ms", w.position_ms());
    }

    #[cfg(unix)]
    #[test]
    fn a_paused_track_resumes_on_the_new_device_where_it_was() {
        let wav = silent_wav("device-paused", 1000);
        let mut w = worker_playing(&wav.to_string_lossy());
        w.pause(true);
        let before = w.position_ms();
        w.set_device("plug:null".into());
        w.pause(false);
        let _ = std::fs::remove_file(wav);
        assert!(before > 0 && w.position_ms() == before, "{before} ms, then {} ms", w.position_ms());
    }

    #[test]
    fn a_seek_not_yet_made_shows_where_it_goes() {
        let engine = Engine::headless();
        let info = StreamInfo {
            sample_rate: 44_100,
            channels: 2,
            length_ms: Some(225_000),
            bitrate_kbps: None,
            codec: "mp3".into(),
            seekable: true,
        };
        engine.simulate(PlayState::Playing, 83_000, &info);
        engine.send(EngineCmd::Seek(10_000));
        assert_eq!((engine.position_ms(), engine.shared().position_ms()), (10_000, 83_000));
    }

    /// Waits until the engine shows `ms`, for up to two seconds.
    #[cfg(unix)]
    fn shows(engine: &Engine, ms: u64) -> bool {
        (0..200).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            engine.position_ms() == ms && engine.shared().position_ms() == ms
        })
    }

    #[cfg(unix)]
    #[test]
    fn seeks_while_paused_show_the_last_one() {
        let sources = Sources::default();
        sources.set_plugins(vec![Arc::new(ShellSource)]);
        let (engine, events) = engine_with(sources);
        engine.send(EngineCmd::Open { id: 1, location: "pipe:head -c 40000000 /dev/zero".into(), start_ms: 0 });
        assert!(matches!(events.recv_timeout(Duration::from_secs(2)), Ok(EngineEvent::Started { id: 1, .. })));
        engine.send(EngineCmd::Pause(true));
        engine.send(EngineCmd::Seek(3_000));
        assert!(shows(&engine, 3_000), "at {} ms", engine.position_ms());
        engine.send(EngineCmd::Seek(5_000));
        assert!(shows(&engine, 5_000), "at {} ms", engine.position_ms());
        engine.send(EngineCmd::Stop);
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
