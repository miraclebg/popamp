//! The sound card: ALSA on Unix, WASAPI on Windows. Each system's `Pcm` is
//! a blocking, interleaved float stream with the same calls.

#[cfg(unix)]
mod alsa;
#[cfg(unix)]
use alsa as system;
#[cfg(unix)]
pub use alsa::{available, AlsaPcm as Pcm};
#[cfg(windows)]
mod wasapi;
#[cfg(windows)]
use wasapi as system;
#[cfg(windows)]
pub use wasapi::{available, WasapiPcm as Pcm};

/// The device the system plays on unless told otherwise.
pub const DEFAULT: &str = "default";

/// A playback device to choose from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// What the settings keep and [`Pcm::open`] takes.
    pub name: String,
    /// What people read.
    pub label: String,
}

/// The playback devices, the system's default first.
pub fn devices() -> Vec<Device> {
    let mut list = vec![Device { name: DEFAULT.into(), label: "Default device".into() }];
    list.extend(system::devices().into_iter().filter(|d| d.name != DEFAULT));
    list
}
