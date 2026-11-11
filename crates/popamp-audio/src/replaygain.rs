//! The ReplayGain scanner: measures the loudness of tracks (EBU R128)
//! and the gain that brings each, and each album, to the ReplayGain 2.0
//! reference. It only reads the files; the values go to Popamp's own
//! store ([`popamp_core::replaygain::Scanned`]).

use crate::decoder::Status;
use crate::engine::open_playback;
use crate::source::{Context, Playback, Sources};
use ebur128::{EbuR128, Mode};
use parking_lot::Mutex;
use popamp_core::replaygain::{Gains, Level, Stamp, REFERENCE_LUFS};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What a track's scan gave.
pub type Outcome = Result<(Stamp, Gains), String>;

/// One track measured.
struct Measured {
    stamp: Stamp,
    /// The folder and album tag: tracks sharing both are one album.
    album: Option<(PathBuf, String)>,
    loudness: EbuR128,
    peak: f32,
}

/// Scans `locations` on a few threads, calling `progress` with the number
/// done after each. Tracks of one album get its album gain too.
pub fn scan(locations: &[String], sources: &Sources, cx: &Context, progress: &(dyn Fn(usize) + Sync)) -> Vec<Outcome> {
    let slots: Vec<Mutex<Option<Result<Measured, String>>>> = locations.iter().map(|_| Mutex::new(None)).collect();
    let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4).min(locations.len());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(location) = locations.get(i) else { break };
                *slots[i].lock() = Some(measure(location, sources, cx));
                progress(done.fetch_add(1, Ordering::Relaxed) + 1);
            });
        }
    });
    let measured = slots.into_iter().map(|m| m.into_inner().unwrap_or_else(|| Err("not scanned".into()))).collect();
    with_album_gains(measured)
}

/// Decodes the whole track at `location`, feeding the loudness meter.
fn measure(location: &str, sources: &Sources, cx: &Context) -> Result<Measured, String> {
    let playback = sources.of(location).open(location, cx)?;
    let path = match &playback {
        Playback::File(path) | Playback::Part { path, .. } => path.clone(),
        Playback::Http(_) | Playback::Pipe(_) => return Err("not a local file".into()),
    };
    let stamp = Stamp::of(&path).ok_or("cannot read the file")?;
    let (mut dec, meta, ..) = open_playback(playback, location, 0, cx)?;
    // Every source gives interleaved stereo.
    let mut loudness = EbuR128::new(2, dec.info().sample_rate, Mode::I | Mode::HISTOGRAM).map_err(|e| e.to_string())?;
    let (mut buf, mut peak) = (Vec::new(), 0f32);
    loop {
        buf.clear();
        let status = dec.next(&mut buf)?;
        loudness.add_frames_f32(&buf).map_err(|e| e.to_string())?;
        peak = buf.iter().fold(peak, |p, s| p.max(s.abs()));
        if let Status::Eof = status {
            break;
        }
    }
    let folder = path.parent().map(PathBuf::from).unwrap_or_default();
    let album = meta.album.filter(|a| !a.trim().is_empty()).map(|a| (folder, a));
    Ok(Measured { stamp, album, loudness, peak })
}

/// The gains of each track, and of the albums among them.
fn with_album_gains(measured: Vec<Result<Measured, String>>) -> Vec<Outcome> {
    let mut albums: HashMap<&(PathBuf, String), Vec<&Measured>> = HashMap::new();
    for m in measured.iter().flatten() {
        if let Some(key) = &m.album {
            albums.entry(key).or_default().push(m);
        }
    }
    let album_levels: HashMap<&(PathBuf, String), Level> = albums
        .into_iter()
        .map(|(key, tracks)| {
            let lufs = EbuR128::loudness_global_multiple(tracks.iter().map(|t| &t.loudness)).unwrap_or(f64::NEG_INFINITY);
            (key, Level { gain_db: gain_db(lufs), peak: tracks.iter().map(|t| t.peak).fold(0.0, f32::max) })
        })
        .collect();
    measured
        .iter()
        .map(|m| {
            let m = m.as_ref().map_err(Clone::clone)?;
            let lufs = m.loudness.loudness_global().unwrap_or(f64::NEG_INFINITY);
            let track = Level { gain_db: gain_db(lufs), peak: m.peak };
            Ok((m.stamp, Gains { track, album: m.album.as_ref().and_then(|k| album_levels.get(k).copied()) }))
        })
        .collect()
}

/// The gain that brings `lufs` to the reference; none for silence.
fn gain_db(lufs: f64) -> f32 {
    if lufs.is_finite() {
        (REFERENCE_LUFS - lufs) as f32
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stereo 16-bit WAV of a 1 kHz sine: `parts` of (seconds, peak).
    fn sine_wav(name: &str, parts: &[(u32, f32)]) -> PathBuf {
        let samples: Vec<i16> = parts
            .iter()
            .flat_map(|&(secs, peak)| {
                (0..44_100 * secs).flat_map(move |i| {
                    let s = (peak * (i as f32 * 1000.0 * std::f32::consts::TAU / 44_100.0).sin() * 32767.0) as i16;
                    [s, s]
                })
            })
            .collect();
        let data_len = samples.len() as u32 * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 0x0002_0001, 44_100, 44_100 * 4, 0x0010_0004] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        samples.iter().for_each(|s| wav.extend_from_slice(&s.to_le_bytes()));
        let dir = std::env::temp_dir().join(format!("popamp-rgscan-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audio.wav");
        std::fs::write(&path, wav).unwrap();
        path
    }

    /// Scans, checking that progress counted every track.
    fn run(locations: &[String]) -> Vec<Outcome> {
        let most = AtomicUsize::new(0);
        let out = scan(locations, &Sources::default(), &Context::default(), &|done| {
            most.fetch_max(done, Ordering::Relaxed);
        });
        assert_eq!(most.into_inner(), locations.len());
        out
    }

    /// The level of a sine that reads `lufs` (K-weighting is ~0 dB at 1 kHz).
    fn peak_of(lufs: f32) -> f32 {
        10f32.powf(lufs / 20.0)
    }

    #[test]
    fn a_track_gets_the_gain_to_the_reference_and_its_peak() {
        let wav = sine_wav("one", &[(3, peak_of(-23.0))]);
        let out = run(&[wav.to_string_lossy().into_owned()]);
        let written = Stamp::of(&wav);
        let _ = std::fs::remove_dir_all(wav.parent().unwrap());
        let (stamp, gains) = out[0].clone().unwrap();
        assert_eq!(Some(stamp), written);
        assert!((gains.track.gain_db - 5.0).abs() < 0.3, "{gains:?}");
        assert!((gains.track.peak - peak_of(-23.0)).abs() < 0.001, "{gains:?}");
        assert_eq!(gains.album, None, "a file with no album tag");
    }

    #[test]
    fn tracks_of_a_cue_album_share_its_gain() {
        let wav = sine_wav("album", &[(3, peak_of(-23.0)), (3, peak_of(-13.0))]);
        let cue = wav.with_file_name("album.cue");
        let sheet = "TITLE \"Loud and quiet\"\nFILE \"audio.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:03:00\n";
        std::fs::write(&cue, sheet).unwrap();
        let cue = cue.to_string_lossy();
        let out = run(&[format!("{cue}#1"), format!("{cue}#2"), "/no/such/file.mp3".into()]);
        let _ = std::fs::remove_dir_all(wav.parent().unwrap());
        let (quiet, loud) = (out[0].clone().unwrap().1, out[1].clone().unwrap().1);
        assert!((quiet.track.gain_db - 5.0).abs() < 0.3 && (loud.track.gain_db + 5.0).abs() < 0.3, "{quiet:?} {loud:?}");
        assert_eq!(quiet.album, loud.album);
        let album = quiet.album.unwrap();
        // The loud half carries the album: about -15.7 LUFS by energy.
        assert!((album.gain_db + 2.3).abs() < 0.4, "{album:?}");
        assert_eq!(album.peak, loud.track.peak);
        assert!(out[2].is_err(), "a missing file fails alone");
    }

    #[test]
    fn silence_gets_no_gain() {
        assert_eq!(gain_db(f64::NEG_INFINITY), 0.0);
        assert_eq!(gain_db(-20.0), 2.0);
    }
}
