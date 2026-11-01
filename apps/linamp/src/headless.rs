//! `/RENDER=<dir>`: draws the windows into PNG files without a display or
//! audio device, for skin previews and pixel comparisons between versions.
//!
//! Each scenario starts from the settings in `<dir>/linamp.ini` (defaults
//! if there is none) and changes only what it shows. The set of states
//! follows Ferramp's `--render` (see THIRD_PARTY.md).

use crate::font::Font;
use crate::model::{AddMode, Model};
use crate::skinwin::SkinWin;
use linamp_audio::{PlayState, StreamInfo};
use linamp_core::config::{Config, Paths, Settings};
use linamp_skin::layout::WindowKey;
use linamp_skin::{Bitmap, Skin};
use std::path::{Path, PathBuf};

/// What the "playing" scenarios pretend to play.
const PLAY_POS_MS: u64 = 83_000;
const FALLBACK_LENGTH_MS: u64 = 225_000;

/// A state that needs only settings: window, file name, what to change.
type Shot = (WindowKey, &'static str, fn(&mut Settings));

const STOPPED: &[Shot] = &[
    (WindowKey::Main, "main-stopped", |_| {}),
    (WindowKey::Eq, "eq-default", |_| {}),
    (WindowKey::Playlist, "playlist-default", |_| {}),
    (WindowKey::Main, "main-toggles", |s| {
        s.shuffle = true;
        s.repeat = true;
        s.volume = 77; // 30 %
        s.balance = -51; // 40 % left
    }),
    // Equalizer values: 0 = +12 dB, 63 = -12 dB, 31 = centre.
    (WindowKey::Eq, "eq-values", |s| {
        s.use_eq = true;
        s.autoload_eq = true;
        s.preamp = 0;
        s.eq_data = "63,0,31,63,0,31,63,0,31,63".into();
    }),
    (WindowKey::Main, "main-shade", |s| s.windowshade = true),
    (WindowKey::Eq, "eq-shade", |s| s.eq_ws = true),
    (WindowKey::Playlist, "playlist-shade", |s| s.pe_ws = true),
    (WindowKey::Playlist, "playlist-small", |s| s.pe_height = 116),
    (WindowKey::Playlist, "playlist-wide", |s| s.pe_width = 450),
];

struct Renderer {
    dir: PathBuf,
    skin: Option<PathBuf>,
    files: Vec<String>,
}

impl Renderer {
    /// A fresh model with the skin and playlist loaded, nothing playing.
    fn model(&self) -> Result<Model, String> {
        let paths = Paths::resolve(Some(self.dir.clone()));
        let config = Config::load(paths.ini_file());
        let mut m = Model::headless(paths, config);
        if let Some(p) = &self.skin {
            m.skin = Skin::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        m.add_now(self.files.clone(), AddMode::Append);
        Ok(m)
    }

    fn save(&self, m: &mut Model, k: WindowKey, name: &str) -> Result<(), String> {
        let (w, h) = crate::bind::window_size(m, k);
        let mut b = Bitmap::new(w, h, 0);
        SkinWin::new(k).draw(m, &mut Font::load(&m.skin.pl.font), &mut b, true);
        let path = self.dir.join(format!("{name}.png"));
        b.save_png(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Renders after changing the settings with `set`.
    fn shot(&self, k: WindowKey, name: &str, set: &fn(&mut Settings)) -> Result<(), String> {
        let mut m = self.model()?;
        set(m.settings_mut());
        self.save(&mut m, k, name)
    }

    fn all(&self) -> Result<(), String> {
        for (k, name, set) in STOPPED {
            self.shot(*k, name, set)?;
        }
        self.playing_states()?;
        self.eq_preset()
    }

    fn playing_states(&self) -> Result<(), String> {
        let mut m = self.playing()?;
        self.save(&mut m, WindowKey::Main, "main-playing")?;
        self.save(&mut m, WindowKey::Playlist, "playlist-playing")?;
        m.settings_mut().sa = 2;
        analyze(&mut m);
        self.save(&mut m, WindowKey::Main, "main-scope")?;
        m.settings_mut().sa = 1;
        m.settings_mut().windowshade = true;
        self.save(&mut m, WindowKey::Main, "main-shade-playing")
    }

    fn eq_preset(&self) -> Result<(), String> {
        let mut m = self.model()?;
        let rock = m.eq_presets.iter().find(|p| p.name.eq_ignore_ascii_case("rock")).cloned();
        if let Some(p) = rock {
            let s = m.settings_mut();
            s.use_eq = true;
            s.preamp = p.preamp as i32;
            s.eq_data = p.bands.map(|b| b.to_string()).join(",");
        }
        self.save(&mut m, WindowKey::Eq, "eq-preset")
    }

    /// The first track "playing" at a fixed position, with a synthetic
    /// signal in the visualizer.
    fn playing(&self) -> Result<Model, String> {
        let mut m = self.model()?;
        let length_ms = m.playlist.entries().first().and_then(|e| e.length_ms).unwrap_or(FALLBACK_LENGTH_MS);
        let info = StreamInfo {
            sample_rate: 44_100,
            channels: 2,
            length_ms: Some(length_ms),
            bitrate_kbps: Some(128),
            codec: "mp3".into(),
            seekable: true,
        };
        m.engine.simulate(PlayState::Playing, PLAY_POS_MS.min(length_ms), &info);
        if !m.playlist.is_empty() {
            m.playlist.set_current(Some(0));
            m.status.playing_id = m.playlist.current_entry().map(|e| e.id);
        }
        m.status.info = Some(info);
        analyze(&mut m);
        Ok(m)
    }
}

/// Feeds 110 Hz + 880 Hz + 5 kHz into the visualizer. Several passes let
/// the bars and peaks settle the way they do after a moment of playback.
fn analyze(m: &mut Model) {
    for (i, s) in m.vis.samples_mut().iter_mut().enumerate() {
        let t = i as f32 / 44_100.0;
        let tone = |hz: f32| (t * hz * std::f32::consts::TAU).sin();
        *s = 0.35 * tone(110.0) + 0.2 * tone(880.0) + 0.1 * tone(5000.0);
    }
    let settings = m.settings().clone();
    for _ in 0..4 {
        m.vis.analyze(&settings, 44_100);
    }
}

pub fn render_all(dir: &Path, skin: Option<PathBuf>, files: Vec<String>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Renderer { dir: dir.to_path_buf(), skin, files }.all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(path: &Path) -> (u32, u32) {
        image::image_dimensions(path).unwrap()
    }

    #[test]
    fn renders_every_window_and_state() {
        let dir = std::env::temp_dir().join(format!("linamp-render-test-{}", std::process::id()));
        render_all(&dir, None, Vec::new()).unwrap();
        assert_eq!(size(&dir.join("main-stopped.png")), (275, 116));
        assert_eq!(size(&dir.join("main-shade.png")), (275, 14));
        assert_eq!(size(&dir.join("eq-default.png")), (275, 116));
        assert_eq!(size(&dir.join("eq-shade.png")), (275, 14));
        assert_eq!(size(&dir.join("playlist-default.png")), (275, 232));
        assert_eq!(size(&dir.join("playlist-wide.png")), (450, 232));
        assert_eq!(size(&dir.join("playlist-shade.png")), (275, 14));
        let read = |n: &str| std::fs::read(dir.join(n)).unwrap();
        assert_ne!(read("main-playing.png"), read("main-stopped.png"), "playing shows time and vis");
        assert_ne!(read("main-scope.png"), read("main-playing.png"), "oscilloscope differs from spectrum");
        assert_ne!(read("eq-values.png"), read("eq-default.png"));
        assert_ne!(read("main-toggles.png"), read("main-stopped.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
