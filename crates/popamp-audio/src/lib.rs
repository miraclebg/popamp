//! Audio side of Popamp: decoding, DSP, sound output and the engine thread.

pub mod cover;
pub mod decoder;
pub mod dsp;
pub mod engine;
pub mod output;
pub mod pipe;
pub mod source;
pub mod stream;
pub mod tags;

pub use decoder::{read_meta, AudioSource, StreamInfo};
pub use dsp::EqSettings;
pub use engine::{Engine, EngineCmd, EngineEvent, PlayState, ReplayGainSettings, Shared};
