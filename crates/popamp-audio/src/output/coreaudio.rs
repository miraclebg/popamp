//! Sound output on macOS, through CoreAudio. Not there yet: only `null`
//! opens, so tests can run; every real device says macOS has no sound
//! yet, and a track fails instead of rushing through on a silent device.

use super::Device;

const NOT_YET: &str = "Popamp cannot play sound on macOS yet";

/// An open, blocking, interleaved playback stream.
pub struct CoreAudioPcm {
    pub rate: u32,
    pub channels: u32,
}

impl CoreAudioPcm {
    /// Opens `null`; any other device is not there yet.
    pub fn open(device: &str, rate: u32, channels: u32, _latency_ms: u32) -> Result<Self, String> {
        match device {
            "null" => Ok(Self { rate, channels }),
            _ => Err(NOT_YET.into()),
        }
    }

    /// Takes the samples at once and plays nothing.
    pub fn write(&mut self, _samples: &[f32]) -> Result<(), String> {
        Ok(())
    }

    /// Frames queued but not yet audible: none.
    pub fn delay(&self) -> u64 {
        0
    }

    /// Pauses or resumes, keeping what is queued.
    pub fn pause(&mut self, _on: bool) -> bool {
        true
    }

    /// Discards queued audio; the next write starts afresh.
    pub fn discard(&mut self) {}

    /// Blocks until queued audio has played.
    pub fn drain(&mut self) {}
}

/// Whether sound can be played: not yet.
pub fn available() -> Result<(), String> {
    Err(NOT_YET.into())
}

/// The playback devices: none yet.
pub fn devices() -> Vec<Device> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_takes_samples_and_plays_nothing() {
        let mut pcm = CoreAudioPcm::open("null", 44_100, 2, 100).unwrap();
        assert_eq!(pcm.write(&[0.0; 512]), Ok(()));
        assert_eq!((pcm.delay(), pcm.rate, pcm.channels), (0, 44_100, 2));
    }

    #[test]
    fn other_devices_say_macos_has_no_sound_yet() {
        let e = CoreAudioPcm::open(super::super::DEFAULT, 44_100, 2, 100).err().unwrap();
        assert!(e.contains("macOS"), "{e}");
        assert!(available().is_err() && devices().is_empty());
    }
}
