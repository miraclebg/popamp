//! Tracker modules (MOD, XM, S3M, IT and the rest) through libopenmpt,
//! loaded at run time like libopus, so building needs no headers.

use crate::decoder::{AudioSource, Status, StreamInfo};
use popamp_core::meta::TrackMeta;
use std::ffi::{c_char, c_double, c_int, c_void, CStr};
use std::path::Path;
use std::sync::OnceLock;

const RATE: u32 = 48_000;
/// Frames rendered per call: about 21 ms.
const BLOCK: usize = 1_024;

type Module = *mut c_void;
type LogFunc = unsafe extern "C" fn(*const c_char, *mut c_void);

struct Lib {
    _lib: libloading::Library,
    create: unsafe extern "C" fn(
        *const c_void,
        usize,
        Option<LogFunc>,
        *mut c_void,
        *const c_void,
        *mut c_void,
        *mut c_int,
        *mut *const c_char,
        *const c_void,
    ) -> Module,
    destroy: unsafe extern "C" fn(Module),
    read: unsafe extern "C" fn(Module, i32, usize, *mut f32) -> usize,
    duration: unsafe extern "C" fn(Module) -> c_double,
    set_position: unsafe extern "C" fn(Module, c_double) -> c_double,
    metadata: unsafe extern "C" fn(Module, *const c_char) -> *const c_char,
    free_string: unsafe extern "C" fn(*const c_char),
    silent: LogFunc,
}

#[cfg(windows)]
const LIB_NAME: &str = "libopenmpt.dll";
#[cfg(not(windows))]
const LIB_NAME: &str = "libopenmpt.so.0";

fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| unsafe {
        let lib =
            libloading::Library::new(LIB_NAME).map_err(|e| format!("tracker modules need libopenmpt: cannot load {LIB_NAME}: {e}"))?;
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| format!("libopenmpt: missing {}: {e}", $name))?
            };
        }
        Ok(Lib {
            create: sym!("openmpt_module_create_from_memory2"),
            destroy: sym!("openmpt_module_destroy"),
            read: sym!("openmpt_module_read_interleaved_float_stereo"),
            duration: sym!("openmpt_module_get_duration_seconds"),
            set_position: sym!("openmpt_module_set_position_seconds"),
            metadata: sym!("openmpt_module_get_metadata"),
            free_string: sym!("openmpt_free_string"),
            silent: sym!("openmpt_log_func_silent"),
            _lib: lib,
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub struct TrackerSource {
    lib: &'static Lib,
    module: Module,
    info: StreamInfo,
}

// The module belongs to this value alone, and only `&mut self` plays it.
unsafe impl Send for TrackerSource {}

impl TrackerSource {
    pub fn open(path: &Path) -> Result<(TrackerSource, TrackMeta), String> {
        let lib = lib()?;
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let module = unsafe {
            let null = std::ptr::null_mut();
            (lib.create)(
                bytes.as_ptr().cast(),
                bytes.len(),
                Some(lib.silent),
                null,
                std::ptr::null(),
                null,
                null.cast(),
                null.cast(),
                std::ptr::null(),
            )
        };
        if module.is_null() {
            return Err(format!("{}: not a module libopenmpt can play", path.display()));
        }
        let mut source = TrackerSource { lib, module, info: StreamInfo::default() };
        let length_ms = (unsafe { (lib.duration)(module) } * 1000.0) as u64;
        let codec = source.metadata("type").to_ascii_uppercase();
        source.info = StreamInfo {
            sample_rate: RATE,
            channels: 2,
            length_ms: Some(length_ms),
            bitrate_kbps: None,
            codec: codec.clone(),
            seekable: true,
        };
        let text = |s: String| (!s.trim().is_empty()).then(|| s.trim().to_string());
        let meta = TrackMeta {
            title: text(source.metadata("title")),
            artist: text(source.metadata("artist")),
            comment: text(source.metadata("message")),
            length_ms: Some(length_ms),
            sample_rate: Some(RATE),
            channels: Some(2),
            codec: Some(codec),
            ..TrackMeta::default()
        };
        Ok((source, meta))
    }

    fn metadata(&self, key: &str) -> String {
        let key = std::ffi::CString::new(key).unwrap_or_default();
        unsafe {
            let value = (self.lib.metadata)(self.module, key.as_ptr());
            if value.is_null() {
                return String::new();
            }
            let text = CStr::from_ptr(value).to_string_lossy().into_owned();
            (self.lib.free_string)(value);
            text
        }
    }
}

impl Drop for TrackerSource {
    fn drop(&mut self) {
        unsafe { (self.lib.destroy)(self.module) };
    }
}

impl AudioSource for TrackerSource {
    fn info(&self) -> &StreamInfo {
        &self.info
    }

    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        let start = out.len();
        out.resize(start + 2 * BLOCK, 0.0);
        let frames = unsafe { (self.lib.read)(self.module, RATE as i32, BLOCK, out[start..].as_mut_ptr()) };
        out.truncate(start + 2 * frames);
        Ok(if frames == 0 { Status::Eof } else { Status::Data })
    }

    fn seek(&mut self, ms: u64) -> Result<u64, String> {
        let reached = unsafe { (self.lib.set_position)(self.module, ms as f64 / 1000.0) };
        Ok((reached * 1000.0) as u64)
    }
}

/// What a module says about itself.
pub fn read_meta(path: &Path) -> Result<TrackMeta, String> {
    TrackerSource::open(path).map(|(_, meta)| meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn a_module_plays_its_note_for_its_length_and_names_itself() {
        let (mut source, meta) = TrackerSource::open(&fixture("sine.mod")).unwrap();
        assert_eq!((meta.title.as_deref(), meta.codec.as_deref(), meta.length_ms), (Some("Popamp test"), Some("MOD"), Some(1_920)));
        let mut out = Vec::new();
        while let Status::Data = source.next(&mut out).unwrap() {}
        let secs = out.len() as f32 / 2.0 / RATE as f32;
        // libopenmpt plays the last tick out after the song's end.
        assert!((1.9..2.1).contains(&secs), "{secs} s");
        let left: Vec<f32> = out[..2 * RATE as usize].iter().step_by(2).copied().collect();
        let hz = left.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((250..270).contains(&hz), "{hz} Hz");
        assert!(left.iter().any(|s| s.abs() > 0.05), "audible");
    }

    #[test]
    fn a_module_seeks() {
        let (mut source, _) = TrackerSource::open(&fixture("sine.mod")).unwrap();
        // A seek lands on a row: here every 120 ms.
        let reached = source.seek(1_000).unwrap();
        assert!(reached.abs_diff(1_000) <= 120, "{reached} ms");
        let mut out = Vec::new();
        while let Status::Data = source.next(&mut out).unwrap() {}
        let left_ms = out.len() as u64 / 2 * 1000 / RATE as u64;
        assert!((1_920 - reached).abs_diff(left_ms) < 120, "{left_ms} ms left after {reached}");
    }

    #[test]
    fn a_file_that_is_not_a_module_says_so() {
        let err = TrackerSource::open(&fixture("silence.flac")).err().unwrap();
        assert!(err.contains("not a module"), "{err}");
    }
}
