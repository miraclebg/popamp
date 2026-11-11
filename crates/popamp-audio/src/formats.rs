//! Which decoder plays a local file: chosen by its extension, Symphonia
//! for every format without a decoder of its own.

use crate::decoder::{AudioSource, Decoder};
use crate::midi::{self, MidiSource};
use crate::source::Context;
use popamp_core::meta::TrackMeta;
use std::path::Path;

enum Kind {
    Midi,
    Common,
}

fn kind(path: &Path) -> Kind {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let is = |list: &[&str]| list.iter().any(|e| e.eq_ignore_ascii_case(ext));
    if is(midi::EXTENSIONS) {
        Kind::Midi
    } else {
        Kind::Common
    }
}

/// Opens a local file to play.
pub fn open(path: &Path, cx: &Context) -> Result<(Box<dyn AudioSource>, TrackMeta), String> {
    match kind(path) {
        Kind::Midi => MidiSource::open(path, &cx.soundfont).map(|(s, m)| (Box::new(s) as Box<dyn AudioSource>, m)),
        Kind::Common => Decoder::open_file(path).map(|(d, m)| (Box::new(d) as Box<dyn AudioSource>, m)),
    }
}

/// Reads tags and stream properties without playing the file.
pub fn read_meta(path: &Path) -> Result<TrackMeta, String> {
    match kind(path) {
        Kind::Midi => midi::read_meta(path),
        Kind::Common => crate::decoder::read_meta(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn each_file_goes_to_the_decoder_of_its_format() {
        let cx = Context { soundfont: fixture("sine.sf2").to_string_lossy().into_owned(), ..Context::default() };
        for (name, codec) in [("a4.mid", "MIDI"), ("tone.opus", "OPUS"), ("silence.flac", "FLAC")] {
            let (source, _) = open(&fixture(name), &cx).unwrap();
            assert_eq!(source.info().codec, codec, "{name}");
            assert_eq!(read_meta(&fixture(name)).unwrap().codec.as_deref(), Some(codec), "{name}");
        }
    }
}
