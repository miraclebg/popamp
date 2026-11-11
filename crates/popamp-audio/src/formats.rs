//! Which decoder plays a local file: chosen by its extension, Symphonia
//! for every format without a decoder of its own.

use crate::decoder::{AudioSource, Decoder, Status, StreamInfo};
use crate::midi::{self, MidiSource};
use crate::source::Context;
use crate::tracker::{self, TrackerSource};
use popamp_core::meta::TrackMeta;
use std::path::Path;

enum Kind {
    Midi,
    Tracker,
    Common,
}

fn kind(path: &Path) -> Kind {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let is = |list: &[&str]| list.iter().any(|e| e.eq_ignore_ascii_case(ext));
    if is(popamp_core::MIDI_EXTENSIONS) {
        Kind::Midi
    } else if is(popamp_core::MODULE_EXTENSIONS) {
        Kind::Tracker
    } else {
        Kind::Common
    }
}

/// Opens a local file to play.
pub fn open(path: &Path, cx: &Context) -> Result<(Box<dyn AudioSource>, TrackMeta), String> {
    match kind(path) {
        Kind::Midi => MidiSource::open(path, &cx.soundfont).map(|(s, m)| (Box::new(s) as Box<dyn AudioSource>, m)),
        Kind::Tracker => TrackerSource::open(path).map(|(s, m)| (Box::new(s) as Box<dyn AudioSource>, m)),
        Kind::Common => Decoder::open_file(path).map(|(d, m)| (Box::new(d) as Box<dyn AudioSource>, m)),
    }
}

/// Opens the part of a local file from `start_ms` to `end_ms` (or its
/// end), as a track of its own: a track of a CUE sheet.
pub fn open_part(path: &Path, start_ms: u64, end_ms: Option<u64>, cx: &Context) -> Result<(Box<dyn AudioSource>, TrackMeta), String> {
    let (inner, mut meta) = open(path, cx)?;
    let part = Part::new(inner, start_ms, end_ms)?;
    meta.length_ms = part.info.length_ms;
    Ok((Box::new(part), meta))
}

/// A stretch of another source; its time starts at zero.
struct Part {
    inner: Box<dyn AudioSource>,
    start_ms: u64,
    end_ms: Option<u64>,
    info: StreamInfo,
    /// Frames given out since `start_ms`.
    frames: u64,
}

impl Part {
    fn new(mut inner: Box<dyn AudioSource>, start_ms: u64, end_ms: Option<u64>) -> Result<Part, String> {
        let reached = if start_ms > 0 { inner.seek(start_ms)? } else { 0 };
        let mut info = inner.info().clone();
        info.length_ms = end_ms.or(info.length_ms).map(|end| end.saturating_sub(start_ms));
        let mut part = Part { inner, start_ms, end_ms, info, frames: 0 };
        part.frames = part.frames_in(reached.saturating_sub(start_ms));
        Ok(part)
    }

    fn frames_in(&self, ms: u64) -> u64 {
        ms * self.info.sample_rate as u64 / 1000
    }
}

impl AudioSource for Part {
    fn info(&self) -> &StreamInfo {
        &self.info
    }

    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        let left = self.end_ms.map(|end| self.frames_in(end - self.start_ms).saturating_sub(self.frames));
        if left == Some(0) {
            return Ok(Status::Eof);
        }
        let start = out.len();
        let status = self.inner.next(out)?;
        let mut got = (out.len() - start) as u64 / 2;
        if let Some(left) = left.filter(|&l| got > l) {
            out.truncate(start + 2 * left as usize);
            got = left;
        }
        self.frames += got;
        Ok(status)
    }

    fn seek(&mut self, ms: u64) -> Result<u64, String> {
        let reached = self.inner.seek(self.start_ms + ms)?.saturating_sub(self.start_ms);
        self.frames = self.frames_in(reached);
        Ok(reached)
    }
}

/// Reads tags and stream properties without playing the file.
pub fn read_meta(path: &Path) -> Result<TrackMeta, String> {
    match kind(path) {
        Kind::Midi => midi::read_meta(path),
        Kind::Tracker => tracker::read_meta(path),
        Kind::Common => crate::decoder::read_meta(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3-second mono WAV at 8 kHz: each second a level of its own, 0.1, 0.3, 0.5.
    fn steps_wav(dir: &Path) -> std::path::PathBuf {
        let samples: Vec<i16> =
            (0..3).flat_map(|s| std::iter::repeat_n((0.1 + 0.2 * s as f32) * 32767.0, 8_000)).map(|v| v as i16).collect();
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut wav = b"RIFF".to_vec();
        wav.extend((36 + data.len() as u32).to_le_bytes());
        wav.extend(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        wav.extend(8_000u32.to_le_bytes());
        wav.extend(16_000u32.to_le_bytes());
        wav.extend(b"\x02\0\x10\0data");
        wav.extend((data.len() as u32).to_le_bytes());
        wav.extend(data);
        let path = dir.join("steps.wav");
        std::fs::write(&path, wav).unwrap();
        path
    }

    fn play_all(source: &mut dyn AudioSource) -> Vec<f32> {
        let mut out = Vec::new();
        while let Status::Data = source.next(&mut out).unwrap() {}
        out
    }

    #[test]
    fn a_part_of_a_file_plays_as_a_track_of_its_own() {
        let dir = std::env::temp_dir().join(format!("popamp-part-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = steps_wav(&dir);
        let cx = Context::default();
        let (mut middle, meta) = open_part(&wav, 1_000, Some(2_000), &cx).unwrap();
        assert_eq!(meta.length_ms, Some(1_000));
        let out = play_all(middle.as_mut());
        assert_eq!(out.len(), 2 * 8_000);
        assert!(out.iter().all(|s| (s - 0.3).abs() < 0.01), "only the second second");
        assert_eq!(middle.seek(500).unwrap(), 500);
        assert_eq!(play_all(middle.as_mut()).len(), 2 * 4_000);
        let (mut last, meta) = open_part(&wav, 2_000, None, &cx).unwrap();
        assert_eq!(meta.length_ms, Some(1_000));
        let out = play_all(last.as_mut());
        assert!(out.len() == 2 * 8_000 && out.iter().all(|s| (s - 0.5).abs() < 0.01));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn each_file_goes_to_the_decoder_of_its_format() {
        let cx = Context { soundfont: fixture("sine.sf2").to_string_lossy().into_owned(), ..Context::default() };
        for (name, codec) in [("a4.mid", "MIDI"), ("sine.mod", "MOD"), ("tone.opus", "OPUS"), ("silence.flac", "FLAC")] {
            let (source, _) = open(&fixture(name), &cx).unwrap();
            assert_eq!(source.info().codec, codec, "{name}");
            assert_eq!(read_meta(&fixture(name)).unwrap().codec.as_deref(), Some(codec), "{name}");
        }
    }
}
