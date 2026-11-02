//! Persistent settings (`popamp.ini`, section `[Popamp]`).
//!
//! Key names follow the original `winamp.ini` keys wherever the setting has
//! the same meaning.

use crate::eqf::Preset;
use crate::ini::Ini;
use std::io;
use std::path::{Path, PathBuf};

pub const SECTION: &str = "Popamp";
pub const DEFAULT_TITLE_FORMAT: &str = "[%artist% - ]$if2(%title%,$filepart(%filename%))";

pub trait IniValue: Sized {
    fn parse_ini(s: &str) -> Option<Self>;
    fn to_ini(&self) -> String;
}

impl IniValue for i32 {
    fn parse_ini(s: &str) -> Option<Self> {
        s.trim().parse().ok()
    }
    fn to_ini(&self) -> String {
        self.to_string()
    }
}

impl IniValue for u32 {
    fn parse_ini(s: &str) -> Option<Self> {
        s.trim().parse().ok()
    }
    fn to_ini(&self) -> String {
        self.to_string()
    }
}

impl IniValue for f32 {
    fn parse_ini(s: &str) -> Option<Self> {
        s.trim().parse().ok().filter(|v: &f32| v.is_finite())
    }
    fn to_ini(&self) -> String {
        self.to_string()
    }
}

impl IniValue for bool {
    fn parse_ini(s: &str) -> Option<Self> {
        match s.trim() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }
    fn to_ini(&self) -> String {
        if *self { "1" } else { "0" }.to_string()
    }
}

impl IniValue for String {
    fn parse_ini(s: &str) -> Option<Self> {
        Some(s.to_string())
    }
    fn to_ini(&self) -> String {
        self.clone()
    }
}

/// How classic skin pixels are scaled to the screen (`wsz_scaling`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scaling {
    /// Whole multiples only; a fractional size rounds down.
    Integer,
    /// Each screen pixel averages the skin pixels it covers: crisp edges,
    /// no uneven pixel steps at fractional sizes.
    Sharp,
    /// Each screen pixel copies the nearest skin pixel.
    Nearest,
}

impl IniValue for Scaling {
    fn parse_ini(s: &str) -> Option<Self> {
        match s.trim() {
            "integer" => Some(Scaling::Integer),
            "sharp" => Some(Scaling::Sharp),
            "nearest" => Some(Scaling::Nearest),
            _ => None,
        }
    }
    fn to_ini(&self) -> String {
        match self {
            Scaling::Integer => "integer",
            Scaling::Sharp => "sharp",
            Scaling::Nearest => "nearest",
        }
        .to_string()
    }
}

/// The window sizes offered, in percent.
pub const ZOOMS: [i32; 7] = [100, 125, 150, 175, 200, 250, 300];

macro_rules! settings {
    ($( $(#[$doc:meta])* $field:ident : $ty:ty = $default:expr ),* $(,)?) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct Settings { $( $(#[$doc])* pub $field: $ty ),* }

        impl Default for Settings {
            fn default() -> Self { Self { $( $field: $default ),* } }
        }

        impl Settings {
            pub fn from_ini(ini: &Ini) -> Self {
                let mut s = Self::default();
                $(
                    if let Some(raw) = ini.get(SECTION, stringify!($field)) {
                        match <$ty as IniValue>::parse_ini(raw) {
                            Some(v) => s.$field = v,
                            None => log::warn!("config: ignoring bad value {:?} for {}", raw, stringify!($field)),
                        }
                    }
                )*
                s.sanitize();
                s
            }

            pub fn write_ini(&self, ini: &mut Ini) {
                $( ini.set(SECTION, stringify!($field), self.$field.to_ini()); )*
            }
        }
    };
}

settings! {
    // Window placement (logical pixels at 1x).
    wx: i32 = 100,
    wy: i32 = 100,
    windowshade: bool = false,
    eq_open: bool = true,
    eq_wx: i32 = 100,
    eq_wy: i32 = 216,
    eq_ws: bool = false,
    pe_open: bool = true,
    pe_wx: i32 = 100,
    pe_wy: i32 = 332,
    pe_width: i32 = 275,
    pe_height: i32 = 232,
    pe_ws: bool = false,
    // Playback.
    /// 0..=255, Winamp scale.
    volume: i32 = 200,
    /// -127..=127, 0 = centre.
    balance: i32 = 0,
    shuffle: bool = false,
    repeat: bool = false,
    /// Auto-advance to the next track (Shift+R toggles "manual advance").
    pladv: bool = true,
    /// Playlist index to resume at.
    plpos: i32 = 0,
    shuffle_morph_rate: i32 = 50,
    buffer_ms: u32 = 150,
    fade_ms: u32 = 1000,
    enqueuedef: bool = false,
    // Window behaviour.
    snap: bool = true,
    snaplen: i32 = 10,
    easymove: bool = true,
    aot: bool = false,
    /// Windows drawn at `zoom` instead of 1x ("Double Size", Ctrl+D).
    dsize: bool = false,
    /// The size "Double Size" switches to, in percent (125..=300).
    zoom: i32 = 200,
    wsz_scaling: Scaling = Scaling::Sharp,
    keeponscreen: bool = true,
    dim_inactive: bool = true,
    // Main window.
    timeleftmode: bool = false,
    autoscrollname: bool = true,
    /// Classic visualizer: 0 = off, 1 = spectrum, 2 = oscilloscope.
    sa: i32 = 1,
    sa_peaks: bool = true,
    /// Peak falloff 0 (slow) ..= 4 (fast).
    sa_peak_falloff: i32 = 1,
    /// Bar falloff 0 (slow) ..= 4 (fast).
    safalloff: i32 = 2,
    /// 0 = normal, 1 = fire, 2 = line.
    sa_style: i32 = 0,
    sa_thick: bool = true,
    /// 0 = dots, 1 = lines, 2 = solid.
    osc_style: i32 = 1,
    vis_fps: u32 = 30,
    // Equalizer.
    use_eq: bool = false,
    autoload_eq: bool = false,
    eq_limiter: bool = true,
    /// 0 = Winamp bands (60Hz..16kHz), 1 = ISO bands (31Hz..16kHz).
    eq_frequencies: i32 = 0,
    /// Ten comma separated band values, 0..=63 where 0 = +12 dB.
    eq_data: String = "31,31,31,31,31,31,31,31,31,31".to_string(),
    /// Preamp, 0..=63 where 0 = +12 dB.
    preamp: i32 = 31,
    /// The user's "Default" equalizer preset (Presets > Load/Save > Default).
    eq_default_data: String = "31,31,31,31,31,31,31,31,31,31".to_string(),
    eq_default_preamp: i32 = 31,
    // Playlist.
    pe_fontsize: i32 = 10,
    shownumsinpl: bool = true,
    zeropadplnum: bool = false,
    titlefmt: String = DEFAULT_TITLE_FORMAT.to_string(),
    // Skins.
    skin: String = String::new(),
    usecursors: bool = true,
    // ReplayGain.
    replaygain: bool = false,
    /// 0 = track, 1 = album.
    replaygain_mode: i32 = 0,
    replaygain_preamp: f32 = 0.0,
    replaygain_non_rg_gain: f32 = -6.0,
    // Network.
    proxy: String = String::new(),
    // YouTube (yt-dlp + ffmpeg).
    ytdlp_path: String = "yt-dlp".to_string(),
    ffmpeg_path: String = "ffmpeg".to_string(),
    ytdlp_format: String = "bestaudio[ext=m4a]/bestaudio".to_string(),
}

impl Settings {
    /// Brings every value into its allowed range.
    pub fn sanitize(&mut self) {
        self.volume = self.volume.clamp(0, 255);
        self.balance = self.balance.clamp(-127, 127);
        self.preamp = self.preamp.clamp(0, 63);
        self.pe_width = self.pe_width.max(275);
        self.pe_height = self.pe_height.max(116);
        self.pe_fontsize = self.pe_fontsize.clamp(6, 40);
        self.snaplen = self.snaplen.clamp(0, 100);
        self.zoom = self.zoom.clamp(125, 300);
        self.vis_fps = self.vis_fps.clamp(1, 70);
        self.sa = self.sa.clamp(0, 2);
        self.buffer_ms = self.buffer_ms.clamp(40, 2000);
    }

    /// The ten EQ bands as 0..=63 values (missing entries are flat).
    pub fn eq_bands(&self) -> [u8; 10] {
        parse_bands(&self.eq_data)
    }

    pub fn set_eq_bands(&mut self, bands: &[u8; 10]) {
        self.eq_data = format_bands(bands);
    }

    /// The user's "Default" equalizer preset; flat until one is saved.
    pub fn eq_default(&self) -> Preset {
        Preset { name: "Default".into(), bands: parse_bands(&self.eq_default_data), preamp: self.eq_default_preamp.clamp(0, 63) as u8 }
    }

    pub fn set_eq_default(&mut self, p: &Preset) {
        self.eq_default_data = format_bands(&p.bands);
        self.eq_default_preamp = p.preamp as i32;
    }
}

/// Ten comma-separated band values (0..=63); missing or bad ones are flat.
fn parse_bands(s: &str) -> [u8; 10] {
    let mut bands = [31u8; 10];
    for (slot, part) in bands.iter_mut().zip(s.split(',')) {
        if let Ok(v) = part.trim().parse::<u8>() {
            *slot = v.min(63);
        }
    }
    bands
}

fn format_bands(bands: &[u8; 10]) -> String {
    bands.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
}

/// Where Popamp keeps its files.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    /// Resolves XDG directories; a `popamp.ini` next to the executable selects
    /// portable mode, and `override_dir` (from `/INIDIR=`) wins over both.
    pub fn resolve(override_dir: Option<PathBuf>) -> Self {
        if let Some(dir) = override_dir {
            return Self { config_dir: dir.clone(), data_dir: dir };
        }
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            if exe_dir.join("popamp.ini").is_file() {
                return Self { config_dir: exe_dir.clone(), data_dir: exe_dir };
            }
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback))
        };
        Self { config_dir: xdg("XDG_CONFIG_HOME", ".config").join("popamp"), data_dir: xdg("XDG_DATA_HOME", ".local/share").join("popamp") }
    }

    pub fn ini_file(&self) -> PathBuf {
        self.config_dir.join("popamp.ini")
    }
    pub fn playlist_file(&self) -> PathBuf {
        self.config_dir.join("popamp.m3u8")
    }
    pub fn bookmarks_file(&self) -> PathBuf {
        self.config_dir.join("bookmarks.m3u8")
    }
    pub fn eq_library(&self) -> PathBuf {
        self.config_dir.join("popamp.q1")
    }
    pub fn eq_auto_library(&self) -> PathBuf {
        self.config_dir.join("popamp_auto.q1")
    }
    pub fn skins_dir(&self) -> PathBuf {
        self.data_dir.join("skins")
    }
}

/// Settings plus the parsed file they came from, so unknown keys survive a save.
pub struct Config {
    pub settings: Settings,
    ini: Ini,
    path: PathBuf,
}

impl Config {
    pub fn load(path: PathBuf) -> Self {
        let ini = match std::fs::read(&path) {
            Ok(bytes) => Ini::parse_bytes(&bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ini::new(),
            Err(e) => {
                log::warn!("config: cannot read {}: {e}", path.display());
                Ini::new()
            }
        };
        Self { settings: Settings::from_ini(&ini), ini, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes atomically (temp file + rename).
    pub fn save(&mut self) -> io::Result<()> {
        self.settings.write_ini(&mut self.ini);
        write_atomic(&self.path, self.ini.to_string_pretty().as_bytes())
    }
}

pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp~");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let mut s = Settings { volume: 123, titlefmt: "%title%".into(), zoom: 150, wsz_scaling: Scaling::Nearest, ..Default::default() };
        s.set_eq_bands(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 63]);
        let mut ini = Ini::new();
        s.write_ini(&mut ini);
        let back = Settings::from_ini(&Ini::parse(&ini.to_string_pretty()));
        assert_eq!(back, s);
        assert_eq!(back.eq_bands()[9], 63);
    }

    #[test]
    fn eq_default_preset_is_flat_until_saved() {
        let mut s = Settings::default();
        let flat = s.eq_default();
        assert_eq!((flat.name.as_str(), flat.bands, flat.preamp), ("Default", [31; 10], 31));
        let mine = Preset { name: "x".into(), bands: [0, 1, 2, 3, 4, 5, 6, 7, 8, 63], preamp: 20 };
        s.set_eq_default(&mine);
        let mut ini = Ini::new();
        s.write_ini(&mut ini);
        let back = Settings::from_ini(&Ini::parse(&ini.to_string_pretty())).eq_default();
        assert_eq!((back.bands, back.preamp), (mine.bands, mine.preamp));
    }

    #[test]
    fn bad_values_fall_back() {
        let ini = Ini::parse("[Popamp]\nvolume=999\nbalance=abc\nshuffle=1\n");
        let s = Settings::from_ini(&ini);
        assert_eq!(s.volume, 255);
        assert_eq!(s.balance, 0);
        assert!(s.shuffle);
    }

    #[test]
    fn save_preserves_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("popamp-cfg-{}", std::process::id()));
        let path = dir.join("popamp.ini");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "[Popamp]\nfuture_key=7\n[Other]\nx=1\n").unwrap();
        let mut cfg = Config::load(path.clone());
        cfg.settings.repeat = true;
        cfg.save().unwrap();
        let ini = Ini::parse(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(ini.get("Popamp", "future_key"), Some("7"));
        assert_eq!(ini.get("Other", "x"), Some("1"));
        assert_eq!(ini.get("Popamp", "repeat"), Some("1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
