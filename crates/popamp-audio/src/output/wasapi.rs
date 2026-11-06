//! WASAPI output in shared mode: the default device, fed 32-bit float at
//! the track's rate, which Windows converts to the mixer's.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVEFORMATEX,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

/// `WAVE_FORMAT_IEEE_FLOAT`, from mmreg.h.
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

/// How long a write waits for room before it checks again.
const WAIT_MS: u32 = 200;

fn err(what: &str, e: windows::core::Error) -> String {
    format!("WASAPI: {what}: {e}")
}

/// The default playback device, with COM started on this thread.
fn default_device() -> Result<IMMDevice, String> {
    // SAFETY: COM calls with valid arguments. A thread already in COM
    // (S_FALSE, or another apartment) keeps what it has.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| err("no device list", e))?;
        devices.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|e| err("no playback device", e))
    }
}

/// An open, blocking, interleaved playback stream.
pub struct WasapiPcm {
    client: IAudioClient,
    render: IAudioRenderClient,
    /// Set by WASAPI when there is room in the buffer.
    room: HANDLE,
    /// The buffer's size in frames.
    size: u32,
    pub rate: u32,
    pub channels: u32,
    /// Whether the device is playing; it starts with the first write.
    running: bool,
    paused: bool,
}

// SAFETY: the COM objects live in the multithreaded apartment, so any
// thread may call them; the engine uses the stream from one at a time.
unsafe impl Send for WasapiPcm {}

impl WasapiPcm {
    /// Opens the default device; `device` names no other yet.
    pub fn open(device: &str, rate: u32, channels: u32, latency_ms: u32) -> Result<Self, String> {
        if device != "default" && !device.is_empty() {
            log::warn!("WASAPI: playing on the default device, not {device:?}");
        }
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
            let client: IAudioClient = default_device()?.Activate(CLSCTX_ALL, None).map_err(|e| err("cannot open the device", e))?;
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
                Ok((size, render)) => Ok(Self { client, render, room, size, rate, channels, running: false, paused: false }),
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

    /// Writes interleaved samples, blocking until the device took them.
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
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

    /// Frames queued but not yet audible.
    pub fn delay(&self) -> u64 {
        self.queued().unwrap_or(0) as u64
    }

    /// Pauses or resumes, keeping what is queued.
    pub fn pause(&mut self, on: bool) -> bool {
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

    /// Discards queued audio; the next write starts afresh.
    pub fn discard(&mut self) {
        self.stop();
    }

    /// Blocks until queued audio has played.
    pub fn drain(&mut self) {
        if self.start().is_ok() {
            while self.queued().is_ok_and(|q| q > 0) {
                self.wait();
            }
        }
        self.stop();
    }
}

impl Drop for WasapiPcm {
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
