//! Audio side of Popamp: decoding, DSP, ALSA output and the engine thread.

pub mod alsa;
pub mod decoder;
pub mod dsp;
pub mod engine;
pub mod pipe;
pub mod source;
pub mod stream;
pub mod youtube;

pub use decoder::{read_meta, AudioSource, StreamInfo};
pub use dsp::EqSettings;
pub use engine::{Engine, EngineCmd, EngineEvent, PlayState, ReplayGainSettings, Shared};
