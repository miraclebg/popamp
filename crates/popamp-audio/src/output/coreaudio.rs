//! Sound output on macOS, through CoreAudio. Devices come from the HAL:
//! settings keep a device's UID, which stays the same across reboots, and
//! people read its name.

use super::Device;
use objc2_core_audio::{
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyDeviceUID, kAudioDevicePropertyLatency, kAudioDevicePropertyNominalSampleRate,
    kAudioDevicePropertySafetyOffset, kAudioDevicePropertyStreams, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject, kAudioObjectUnknown, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
};
use objc2_core_foundation::{CFRetained, CFString};
use std::ptr::NonNull;

/// An open, blocking, interleaved playback stream.
pub struct CoreAudioPcm {
    pub rate: u32,
    pub channels: u32,
}

impl CoreAudioPcm {
    /// Opens the device with that UID, the default one, or `null`.
    pub fn open(device: &str, rate: u32, channels: u32, _latency_ms: u32) -> Result<Self, String> {
        match device {
            "null" => Ok(Self { rate, channels }),
            super::DEFAULT => Err("not yet".into()),
            uid => match device_by_uid(uid) {
                Some(_) => Err("not yet".into()),
                None => Err(format!("no output device {uid}")),
            },
        }
    }

    /// Writes interleaved samples, blocking until the device took them.
    pub fn write(&mut self, _samples: &[f32]) -> Result<(), String> {
        Ok(())
    }

    /// Frames queued but not yet audible.
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

/// Whether there is a device to play on.
pub fn available() -> Result<(), String> {
    default_output().map(|_| ()).ok_or_else(|| "no sound output device".into())
}

/// The devices that can play, by name, each kept by its UID.
pub fn devices() -> Vec<Device> {
    output_devices().into_iter().map(|(_, uid, name)| Device { name: uid, label: name }).collect()
}

/// The address of property `selector` in `scope`.
fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain }
}

/// A fixed-size property of `object`.
fn property<T: Copy + Default>(object: AudioObjectID, selector: u32, scope: u32) -> Option<T> {
    let address = address(selector, scope);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` is a `T` of `size` bytes, and the HAL writes no more.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0).then_some(value)
}

/// A string property of `object`.
fn string_property(object: AudioObjectID, selector: u32) -> Option<String> {
    let address = address(selector, kAudioObjectPropertyScopeGlobal);
    let mut string: *const CFString = std::ptr::null();
    let mut size = size_of::<*const CFString>() as u32;
    // SAFETY: the HAL writes one CFString reference into `string`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut string).cast(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: the HAL hands over a reference of its own (+1), released here.
    let string = unsafe { CFRetained::from_raw(NonNull::new(string.cast_mut())?) };
    Some(string.to_string())
}

/// How many bytes property `selector` of `object` has in `scope`.
fn property_size(object: AudioObjectID, selector: u32, scope: u32) -> u32 {
    let address = address(selector, scope);
    let mut size = 0;
    // SAFETY: only writes `size`.
    let status = unsafe { AudioObjectGetPropertyDataSize(object, NonNull::from(&address), 0, std::ptr::null(), NonNull::from(&mut size)) };
    if status == 0 {
        size
    } else {
        0
    }
}

/// The system's default output device, if there is one.
fn default_output() -> Option<AudioObjectID> {
    property(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal)
        .filter(|&id| id != kAudioObjectUnknown)
}

/// Every device that can play: its ID, UID and name.
fn output_devices() -> Vec<(AudioObjectID, String, String)> {
    let system = kAudioObjectSystemObject as AudioObjectID;
    let address = address(kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal);
    let mut size = property_size(system, kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal);
    let mut ids = vec![kAudioObjectUnknown; size as usize / size_of::<AudioObjectID>()];
    let Some(out) = NonNull::new(ids.as_mut_ptr()) else { return Vec::new() };
    // SAFETY: `ids` has room for `size` bytes of device IDs.
    if unsafe { AudioObjectGetPropertyData(system, NonNull::from(&address), 0, std::ptr::null(), NonNull::from(&mut size), out.cast()) }
        != 0
    {
        return Vec::new();
    }
    ids.truncate(size as usize / size_of::<AudioObjectID>());
    ids.into_iter()
        .filter(|&id| property_size(id, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput) > 0)
        .filter_map(|id| Some((id, string_property(id, kAudioDevicePropertyDeviceUID)?, string_property(id, kAudioObjectPropertyName)?)))
        .collect()
}

/// The device with UID `uid`, if it is there and can play.
fn device_by_uid(uid: &str) -> Option<AudioObjectID> {
    output_devices().into_iter().find(|(_, u, _)| u == uid).map(|(id, _, _)| id)
}

/// How many frames `device` holds before they are heard (its latency, its
/// safety offset and its buffer), at its own rate, which comes second.
#[allow(dead_code)] // the stream uses it next
fn device_latency(device: AudioObjectID) -> (u32, f64) {
    let out = |selector| property::<u32>(device, selector, kAudioObjectPropertyScopeOutput).unwrap_or(0);
    let buffer = property::<u32>(device, kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal).unwrap_or(0);
    let rate = property::<f64>(device, kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal).unwrap_or(0.0);
    (out(kAudioDevicePropertyLatency) + out(kAudioDevicePropertySafetyOffset) + buffer, rate)
}

/// `device_frames` at `device_rate`, counted at `rate`; 0 for a device
/// that says nothing of its rate.
fn to_stream_frames(device_frames: u32, device_rate: f64, rate: u32) -> u64 {
    if device_rate <= 0.0 {
        return 0;
    }
    (device_frames as f64 * rate as f64 / device_rate).round() as u64
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

    /// Whether this Mac can play sound; hardware tests skip themselves otherwise.
    fn speakers() -> bool {
        let there = default_output().is_some();
        if !there {
            eprintln!("no sound output device: skipped");
        }
        there
    }

    #[test]
    fn device_frames_count_at_the_streams_rate() {
        assert_eq!(to_stream_frames(480, 48_000.0, 44_100), 441);
        assert_eq!(to_stream_frames(512, 44_100.0, 44_100), 512);
        assert_eq!(to_stream_frames(0, 0.0, 44_100), 0); // a device that says nothing
    }

    #[test]
    fn devices_have_a_name_people_read_and_a_uid_settings_keep() {
        if !speakers() {
            return;
        }
        let list = devices();
        assert!(!list.is_empty() && available().is_ok());
        assert!(list.iter().all(|d| !d.label.is_empty() && !d.name.is_empty() && d.name != super::super::DEFAULT));
        let mut uids: Vec<&str> = list.iter().map(|d| d.name.as_str()).collect();
        uids.dedup();
        assert_eq!(uids.len(), list.len(), "{list:?}");
    }

    #[test]
    fn a_device_opens_by_its_uid() {
        if !speakers() {
            return;
        }
        let first = devices().remove(0);
        assert!(CoreAudioPcm::open(&first.name, 48_000, 2, 150).is_ok(), "{first:?}");
    }

    #[test]
    fn an_unknown_device_is_named_in_the_error() {
        let e = CoreAudioPcm::open("popamp-no-such-uid", 48_000, 2, 150).err().unwrap();
        assert!(e.contains("popamp-no-such-uid"), "{e}");
    }
}
