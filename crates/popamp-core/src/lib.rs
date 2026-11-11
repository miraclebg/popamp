//! Toolkit- and audio-independent pieces of Popamp.

pub mod atf;
pub mod command;
pub mod config;
pub mod eqf;
pub mod ini;
pub mod meta;
pub mod playlist;
pub mod plformats;
pub mod text;
pub mod version;

pub use command::Command;

/// File extensions (lower-case) Popamp tries to play.
/// The files Symphonia (and libopus) decode.
pub const COMMON_EXTENSIONS: &[&str] = &[
    "mp3", "mp2", "mp1", "aac", "m4a", "m4b", "mp4", "alac", "flac", "ogg", "oga", "opus", "wav", "wave", "aif", "aiff", "aifc", "caf",
    "mka", "mkv", "webm",
];

/// The files the SoundFont synthesizer plays.
pub const MIDI_EXTENSIONS: &[&str] = &["mid", "midi", "kar", "rmi"];

/// The common tracker module types; libopenmpt reads a few more.
pub const MODULE_EXTENSIONS: &[&str] = &[
    "mod", "xm", "s3m", "it", "mptm", "stm", "669", "mtm", "med", "okt", "far", "ult", "amf", "dsm", "psm", "umx", "dmf", "mdl", "ptm",
    "gdm", "imf", "j2b",
];

/// Every audio file extension Popamp plays.
pub fn audio_extensions() -> impl Iterator<Item = &'static str> {
    COMMON_EXTENSIONS.iter().chain(MIDI_EXTENSIONS).chain(MODULE_EXTENSIONS).copied()
}

pub fn is_audio_file(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| audio_extensions().any(|e| e.eq_ignore_ascii_case(ext)))
}
