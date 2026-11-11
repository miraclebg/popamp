//! Audio side of Popamp: decoding, DSP, sound output and the engine thread.

pub mod cover;
pub mod decoder;
pub mod dsp;
pub mod engine;
mod formats;
mod midi;
mod opus;
pub mod output;
pub mod pipe;
pub mod source;
pub mod stream;
pub mod tags;

pub use decoder::{AudioSource, StreamInfo};
pub use dsp::EqSettings;
pub use engine::{Engine, EngineCmd, EngineEvent, PlayState, ReplayGainSettings, Shared};
pub use formats::read_meta;
