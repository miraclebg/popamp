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
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "mp2", "mp1", "aac", "m4a", "m4b", "mp4", "alac", "flac", "ogg", "oga", "opus", "wav", "wave", "aif", "aiff", "aifc", "caf",
    "mka", "mkv", "webm", "mid", "midi", "kar", "rmi",
];

pub fn is_audio_file(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| AUDIO_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}
