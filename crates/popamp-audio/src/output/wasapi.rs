//! WASAPI output in shared mode: a device picked by its name (or the
//! default one), fed 32-bit float at the track's rate, which Windows
//! converts to the mixer's. The device `null` plays nothing, like ALSA's.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE,
    WAVEFORMATEX,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use super::{Device, DEFAULT};

/// `WAVE_FORMAT_IEEE_FLOAT`, from mmreg.h.
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

/// How long a write waits for room before it checks again.
const WAIT_MS: u32 = 200;

fn err(what: &str, e: windows::core::Error) -> String {
    format!("WASAPI: {what}: {e}")
}

/// The device list, with COM started on this thread.
fn enumerator() -> Result<IMMDeviceEnumerator, String> {
    // SAFETY: COM calls with valid arguments. A thread already in COM
    // (S_FALSE, or another apartment) keeps what it has.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| err("no device list", e))
    }
}

fn default_device() -> Result<IMMDevice, String> {
    // SAFETY: a call on the enumerator.
    unsafe { enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole) }.map_err(|e| err("no playback device", e))
}

/// The playback devices plugged in, with their names.
fn endpoints() -> Result<Vec<(String, IMMDevice)>, String> {
    // SAFETY: calls on the collection, within its count.
    unsafe {
        let all = enumerator()?.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(|e| err("no device list", e))?;
        let count = all.GetCount().map_err(|e| err("no device list", e))?;
        Ok((0..count).filter_map(|i| all.Item(i).ok()).filter_map(|d| Some((friendly_name(&d)?, d))).collect())
    }
}

/// The name Windows shows for a device, like "Speakers (Realtek Audio)".
fn friendly_name(device: &IMMDevice) -> Option<String> {
    // SAFETY: the property value is cleared, and its string freed, once read.
    unsafe {
        let mut value = device.OpenPropertyStore(STGM_READ).ok()?.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let text = PropVariantToStringAlloc(&value).ok();
        let _ = PropVariantClear(&mut value);
        let text = text?;
        let name = text.to_string().ok();
        CoTaskMemFree(Some(text.0.cast()));
        name
    }
}

/// The device named `name`, or the default one for [`DEFAULT`].
fn device(name: &str) -> Result<IMMDevice, String> {
    if name == DEFAULT || name.is_empty() {
        return default_device();
    }
    endpoints()?.into_iter().find(|(n, _)| n == name).map(|(_, d)| d).ok_or_else(|| format!("WASAPI: no device {name:?}"))
}

/// The playback devices, by the names Windows shows. On a thread of its
/// own, like [`available`].
pub fn devices() -> Vec<Device> {
    let names = || endpoints().map(|list| list.into_iter().map(|(name, _)| name).collect::<Vec<_>>());
    let found = std::thread::spawn(names).join().unwrap_or_else(|_| Err("WASAPI: listing failed".into()));
    match found {
        Ok(names) => names.into_iter().map(|name| Device { label: name.clone(), name }).collect(),
        Err(e) => {
            log::warn!("{e}");
            Vec::new()
        }
    }
}

/// An open, blocking, interleaved playback stream.
pub struct WasapiPcm {
    /// The device's stream; none for `null`.
    stream: Option<Stream>,
    pub rate: u32,
    pub channels: u32,
}

impl WasapiPcm {
    /// Opens the device of that name, the default one, or `null`.
    pub fn open(device: &str, rate: u32, channels: u32, latency_ms: u32) -> Result<Self, String> {
        let stream = match device {
            "null" => None,
            _ => Some(Stream::open(device, rate, channels, latency_ms)?),
        };
        Ok(Self { stream, rate, channels })
    }

    /// Writes interleaved samples, blocking until the device took them.
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        self.stream.as_mut().map_or(Ok(()), |s| s.write(samples))
    }

    /// Frames queued but not yet audible.
    pub fn delay(&self) -> u64 {
        self.stream.as_ref().map_or(0, |s| s.queued().unwrap_or(0) as u64)
    }

    /// Pauses or resumes, keeping what is queued.
    pub fn pause(&mut self, on: bool) -> bool {
        self.stream.as_mut().is_none_or(|s| s.pause(on))
    }

    /// Discards queued audio; the next write starts afresh.
    pub fn discard(&mut self) {
        if let Some(s) = &mut self.stream {
            s.stop();
        }
    }

    /// Blocks until queued audio has played.
    pub fn drain(&mut self) {
        if let Some(s) = &mut self.stream {
            s.drain();
        }
    }
}

/// A device's shared-mode stream.
struct Stream {
    client: IAudioClient,
    render: IAudioRenderClient,
    /// Set by WASAPI when there is room in the buffer.
    room: HANDLE,
    /// The buffer's size in frames.
    size: u32,
    channels: u32,
    /// Whether the device is playing; it starts with the first write.
    running: bool,
    paused: bool,
}

// SAFETY: the COM objects live in the multithreaded apartment, so any
// thread may call them; the engine uses the stream from one at a time.
unsafe impl Send for Stream {}

impl Stream {
    fn open(name: &str, rate: u32, channels: u32, latency_ms: u32) -> Result<Self, String> {
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: channels as u16,
            nSamplesPerSec: rate,
            nAvgBytesPerSec: rate * channels * 4,
            nBlockAlign: (channels * 4) as u16,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        // SAFETY: COM calls on objects created here, with a format that
        // lives through Initialize.
        unsafe {
            let client: IAudioClient = device(name)?.Activate(CLSCTX_ALL, None).map_err(|e| err("cannot open the device", e))?;
            let duration = latency_ms.max(10) as i64 * 10_000;
            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, flags, duration, 0, &format, None)
                .map_err(|e| err(&format!("cannot play {rate} Hz / {channels} ch"), e))?;
            let room = CreateEventW(None, false, false, None).map_err(|e| err("no event", e))?;
            let rest = (|| {
                client.SetEventHandle(room)?;
                Ok::<_, windows::core::Error>((client.GetBufferSize()?, client.GetService::<IAudioRenderClient>()?))
            })();
            match rest {
                Ok((size, render)) => Ok(Self { client, render, room, size, channels, running: false, paused: false }),
                Err(e) => {
                    let _ = CloseHandle(room);
                    Err(err("cannot set up the stream", e))
                }
            }
        }
    }

    /// Frames in the device's buffer, not yet played.
    fn queued(&self) -> Result<u32, String> {
        // SAFETY: a call on our client.
        unsafe { self.client.GetCurrentPadding() }.map_err(|e| err("lost the device", e))
    }

    fn start(&mut self) -> Result<(), String> {
        if !self.running && !self.paused {
            // SAFETY: a call on our client.
            unsafe { self.client.Start() }.map_err(|e| err("cannot start", e))?;
            self.running = true;
        }
        Ok(())
    }

    /// Stops the device and empties its buffer.
    fn stop(&mut self) {
        // SAFETY: calls on our client; Reset needs it stopped.
        unsafe {
            let _ = self.client.Stop();
            let _ = self.client.Reset();
        }
        self.running = false;
    }

    /// Waits for room in the buffer (or the time to check again).
    fn wait(&self) {
        // SAFETY: our event handle.
        unsafe { WaitForSingleObject(self.room, WAIT_MS) };
    }

    fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        let ch = self.channels as usize;
        let mut rest = samples;
        while rest.len() >= ch {
            let room = self.size - self.queued()?.min(self.size);
            if room == 0 {
                // A full buffer plays once started.
                self.start()?;
                self.wait();
                continue;
            }
            let n = room.min((rest.len() / ch) as u32);
            let (now, later) = rest.split_at(n as usize * ch);
            // SAFETY: GetBuffer gives room for `n` frames of the stream's
            // format, which is `ch` f32 samples each.
            unsafe {
                let buf = self.render.GetBuffer(n).map_err(|e| err("lost the device", e))?;
                std::ptr::copy_nonoverlapping(now.as_ptr(), buf.cast::<f32>(), now.len());
                self.render.ReleaseBuffer(n, 0).map_err(|e| err("lost the device", e))?;
            }
            rest = later;
            self.start()?;
        }
        Ok(())
    }

    fn pause(&mut self, on: bool) -> bool {
        self.paused = on;
        if on {
            // SAFETY: a call on our client.
            let _ = unsafe { self.client.Stop() };
            self.running = false;
            true
        } else {
            self.start().is_ok()
        }
    }

    fn drain(&mut self) {
        if self.start().is_ok() {
            while self.queued().is_ok_and(|q| q > 0) {
                self.wait();
            }
        }
        self.stop();
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: our client and event, closed once.
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.room);
        }
    }
}

/// Checks that there is a playback device (for diagnostics). On a thread
/// of its own: the caller's thread keeps its COM apartment, which the
/// window system wants single-threaded.
pub fn available() -> Result<(), String> {
    std::thread::spawn(|| default_device().map(|_| ())).join().unwrap_or_else(|_| Err("WASAPI: check failed".into()))
}
