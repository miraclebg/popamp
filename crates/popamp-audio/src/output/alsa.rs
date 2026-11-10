//! Minimal ALSA PCM output, loaded at runtime from `libasound.so.2`.
//!
//! Loading at runtime means building needs no ALSA headers, and on PipeWire
//! or PulseAudio systems the `default` device goes through their ALSA
//! plug-ins, which also resample when needed.

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CStr, CString};
use std::sync::OnceLock;

use super::Device;

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
    name_hint: unsafe extern "C" fn(c_int, *const c_char, *mut *mut *mut c_void) -> c_int,
    get_hint: unsafe extern "C" fn(*const c_void, *const c_char) -> *mut c_char,
    free_hint: unsafe extern "C" fn(*mut *mut c_void) -> c_int,
}

extern "C" {
    /// The C library's, which frees the strings ALSA's hints give out.
    fn free(p: *mut c_void);
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
            name_hint: sym!("snd_device_name_hint"),
            get_hint: sym!("snd_device_name_get_hint"),
            free_hint: sym!("snd_device_name_free_hint"),
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

/// The playback devices ALSA names, without the ones nobody picks by hand
/// (raw hardware, mixing and conversion plug-ins).
pub fn devices() -> Vec<Device> {
    let Ok(lib) = lib() else { return Vec::new() };
    let mut hints: *mut *mut c_void = std::ptr::null_mut();
    // SAFETY: valid out-pointer; card -1 asks for all cards.
    if unsafe { (lib.name_hint)(-1, c"pcm".as_ptr(), &mut hints) } < 0 {
        return Vec::new();
    }
    let mut list = Vec::new();
    // SAFETY: the hint array ends with a null pointer and lives until freed.
    unsafe {
        let mut at = hints;
        while !(*at).is_null() {
            let (name, desc, io) = (hint(lib, *at, c"NAME"), hint(lib, *at, c"DESC"), hint(lib, *at, c"IOID"));
            if let Some(name) = name.filter(|n| listed(n) && io.as_deref() != Some("Input")) {
                list.push(Device { label: label(&name, desc.as_deref()), name });
            }
            at = at.add(1);
        }
        (lib.free_hint)(hints);
    }
    list
}

/// One field of a device hint.
///
/// # Safety
/// `h` is a hint from `snd_device_name_hint`.
unsafe fn hint(lib: &Lib, h: *const c_void, id: &CStr) -> Option<String> {
    let p = unsafe { (lib.get_hint)(h, id.as_ptr()) };
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { free(p.cast()) };
    Some(s)
}

/// Sound servers, and each card's outputs with conversions.
fn listed(name: &str) -> bool {
    match name.split_once(':') {
        Some((kind, _)) => matches!(kind, "sysdefault" | "front" | "hdmi" | "iec958"),
        None => matches!(name, "pipewire" | "pulse" | "jack"),
    }
}

/// "Card, device (what it is)" from ALSA's two-line description.
fn label(name: &str, desc: Option<&str>) -> String {
    let mut lines = desc.unwrap_or("").lines().map(str::trim).filter(|l| !l.is_empty());
    match (lines.next(), lines.next()) {
        (Some(card), Some(what)) => format!("{card} ({what})"),
        (Some(card), None) => card.to_string(),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_servers_and_card_outputs_only() {
        let shown: Vec<_> = [
            "pipewire",
            "pulse",
            "lavrate",
            "null",
            "hw:CARD=Audio,DEV=0",
            "plughw:CARD=Audio,DEV=0",
            "sysdefault:CARD=Audio",
            "front:CARD=Audio,DEV=0",
            "hdmi:CARD=NVidia,DEV=0",
            "dmix:CARD=NVidia,DEV=3",
            "surround51:CARD=Audio,DEV=0",
        ]
        .into_iter()
        .filter(|n| listed(n))
        .collect();
        assert_eq!(shown, ["pipewire", "pulse", "sysdefault:CARD=Audio", "front:CARD=Audio,DEV=0", "hdmi:CARD=NVidia,DEV=0"]);
    }

    #[test]
    fn labels_join_the_description_lines() {
        assert_eq!(
            label("hdmi:CARD=NVidia,DEV=0", Some("HDA NVidia, HDMI 0\nHDMI Audio Output")),
            "HDA NVidia, HDMI 0 (HDMI Audio Output)"
        );
        assert_eq!(label("pipewire", Some("PipeWire Sound Server")), "PipeWire Sound Server");
        assert_eq!(label("jack", None), "jack");
    }
}
