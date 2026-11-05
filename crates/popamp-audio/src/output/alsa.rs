//! Minimal ALSA PCM output, loaded at runtime from `libasound.so.2`.
//!
//! Loading at runtime means building needs no ALSA headers, and on PipeWire
//! or PulseAudio systems the `default` device goes through their ALSA
//! plug-ins, which also resample when needed.

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CStr, CString};
use std::sync::OnceLock;

type Pcm = *mut c_void;

struct Lib {
    _lib: libloading::Library,
    open: unsafe extern "C" fn(*mut Pcm, *const c_char, c_int, c_int) -> c_int,
    set_params: unsafe extern "C" fn(Pcm, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int,
    writei: unsafe extern "C" fn(Pcm, *const c_void, c_ulong) -> c_long,
    recover: unsafe extern "C" fn(Pcm, c_int, c_int) -> c_int,
    delay: unsafe extern "C" fn(Pcm, *mut c_long) -> c_int,
    drop: unsafe extern "C" fn(Pcm) -> c_int,
    drain: unsafe extern "C" fn(Pcm) -> c_int,
    prepare: unsafe extern "C" fn(Pcm) -> c_int,
    pause: unsafe extern "C" fn(Pcm, c_int) -> c_int,
    close: unsafe extern "C" fn(Pcm) -> c_int,
    strerror: unsafe extern "C" fn(c_int) -> *const c_char,
}

const STREAM_PLAYBACK: c_int = 0;
const ACCESS_RW_INTERLEAVED: c_int = 3;
const FORMAT_S16_LE: c_int = 2;
const FORMAT_FLOAT_LE: c_int = 14;

fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let lib = libloading::Library::new("libasound.so.2").map_err(|e| format!("cannot load libasound.so.2: {e}"))?;
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("libasound: missing {}: {e}", $name))?
            };
        }
        Ok(Lib {
            open: sym!("snd_pcm_open"),
            set_params: sym!("snd_pcm_set_params"),
            writei: sym!("snd_pcm_writei"),
            recover: sym!("snd_pcm_recover"),
            delay: sym!("snd_pcm_delay"),
            drop: sym!("snd_pcm_drop"),
            drain: sym!("snd_pcm_drain"),
            prepare: sym!("snd_pcm_prepare"),
            pause: sym!("snd_pcm_pause"),
            close: sym!("snd_pcm_close"),
            strerror: sym!("snd_strerror"),
            _lib: lib,
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

fn err_str(lib: &Lib, code: c_int) -> String {
    // SAFETY: snd_strerror returns a static NUL-terminated string.
    unsafe { CStr::from_ptr((lib.strerror)(code)).to_string_lossy().into_owned() }
}

/// An open, blocking, interleaved stereo playback stream.
pub struct AlsaPcm {
    pcm: Pcm,
    lib: &'static Lib,
    pub rate: u32,
    pub channels: u32,
    float: bool,
    scratch: Vec<i16>,
}

// SAFETY: the handle is only used from one thread at a time (owned by the engine).
unsafe impl Send for AlsaPcm {}

impl AlsaPcm {
    pub fn open(device: &str, rate: u32, channels: u32, latency_ms: u32) -> Result<Self, String> {
        let lib = lib()?;
        let name = CString::new(device).map_err(|_| "bad device name".to_string())?;
        let mut pcm: Pcm = std::ptr::null_mut();
        // SAFETY: valid out-pointer and NUL-terminated name.
        let rc = unsafe { (lib.open)(&mut pcm, name.as_ptr(), STREAM_PLAYBACK, 0) };
        if rc < 0 {
            return Err(format!("cannot open ALSA device '{device}': {}", err_str(lib, rc)));
        }
        let latency_us = latency_ms.saturating_mul(1000);
        let mut float = true;
        // SAFETY: pcm is a freshly opened handle.
        let mut rc = unsafe { (lib.set_params)(pcm, FORMAT_FLOAT_LE, ACCESS_RW_INTERLEAVED, channels, rate, 1, latency_us) };
        if rc < 0 {
            float = false;
            rc = unsafe { (lib.set_params)(pcm, FORMAT_S16_LE, ACCESS_RW_INTERLEAVED, channels, rate, 1, latency_us) };
        }
        if rc < 0 {
            unsafe { (lib.close)(pcm) };
            return Err(format!("ALSA cannot play {rate} Hz / {channels} ch: {}", err_str(lib, rc)));
        }
        Ok(Self { pcm, lib, rate, channels, float, scratch: Vec::new() })
    }

    /// Writes interleaved samples, blocking until the device accepted them.
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        let ch = self.channels as usize;
        let mut frames = samples.len() / ch;
        let mut offset = 0usize;
        if !self.float {
            self.scratch.clear();
            self.scratch.extend(samples.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
        }
        let mut retries = 0;
        while frames > 0 {
            let ptr: *const c_void =
                if self.float { samples[offset * ch..].as_ptr().cast() } else { self.scratch[offset * ch..].as_ptr().cast() };
            // SAFETY: ptr points at `frames * ch` valid samples of the configured format.
            let n = unsafe { (self.lib.writei)(self.pcm, ptr, frames as c_ulong) };
            if n < 0 {
                let rc = unsafe { (self.lib.recover)(self.pcm, n as c_int, 1) };
                retries += 1;
                if rc < 0 || retries > 10 {
                    return Err(format!("ALSA write failed: {}", err_str(self.lib, n as c_int)));
                }
                continue;
            }
            frames -= n as usize;
            offset += n as usize;
        }
        Ok(())
    }

    /// Frames queued but not yet audible.
    pub fn delay(&self) -> u64 {
        let mut d: c_long = 0;
        // SAFETY: valid handle and out-pointer.
        let rc = unsafe { (self.lib.delay)(self.pcm, &mut d) };
        if rc < 0 {
            0
        } else {
            d.max(0) as u64
        }
    }

    /// Hardware pause; returns false if the device can't pause.
    pub fn pause(&mut self, on: bool) -> bool {
        // SAFETY: valid handle.
        unsafe { (self.lib.pause)(self.pcm, on as c_int) >= 0 }
    }

    /// Discards queued audio and makes the stream ready for new data.
    pub fn discard(&mut self) {
        // SAFETY: valid handle.
        unsafe {
            (self.lib.drop)(self.pcm);
            (self.lib.prepare)(self.pcm);
        }
    }

    /// Blocks until queued audio has played.
    pub fn drain(&mut self) {
        // SAFETY: valid handle.
        unsafe {
            (self.lib.drain)(self.pcm);
            (self.lib.prepare)(self.pcm);
        }
    }
}

impl Drop for AlsaPcm {
    fn drop(&mut self) {
        // SAFETY: handle is owned and closed exactly once.
        unsafe { (self.lib.close)(self.pcm) };
    }
}

/// Checks whether ALSA can be loaded (for diagnostics).
pub fn available() -> Result<(), String> {
    lib().map(|_| ())
}
