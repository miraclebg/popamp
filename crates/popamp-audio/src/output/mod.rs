//! The sound card: ALSA on Unix, WASAPI on Windows. Each system's `Pcm` is
//! a blocking, interleaved float stream with the same calls.

#[cfg(unix)]
mod alsa;
#[cfg(unix)]
pub use alsa::{available, AlsaPcm as Pcm};
#[cfg(windows)]
mod wasapi;
#[cfg(windows)]
pub use wasapi::{available, WasapiPcm as Pcm};
