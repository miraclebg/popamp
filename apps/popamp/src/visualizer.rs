//! The window of a visualizer plug-in, in a build with the `visualizers`
//! feature. Drawing happens on a thread of its own (see `popamp_visual`).

use crate::model::{AppEvent, Events};
use popamp_audio::{PlayState, Shared};
use popamp_plugins::host::{Plugin, Visual};
use popamp_visual::{Control, Feed, SPECTRUM_LEN, WAVE_LEN};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window};

const FLOOR_DB: f32 = -70.0;

pub struct VisualizerWindow {
    pub window: Arc<Window>,
    control: Arc<Control>,
    full: Arc<AtomicBool>,
}

impl VisualizerWindow {
    pub fn open(el: &ActiveEventLoop, plugin: &Arc<Plugin>, shared: Arc<Shared>, events: Events) -> Result<VisualizerWindow, String> {
        let visual = plugin.visual()?;
        let attrs = Window::default_attributes().with_title(plugin.manifest().name.clone()).with_inner_size(LogicalSize::new(960, 540));
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let control = Control::new(pixels(window.inner_size()));
        let id = plugin.manifest().id.clone();
        let full = Arc::new(AtomicBool::new(false));
        keep_screen_on_while_watched(&control, &full, shared.clone());
        let feed = Box::new(Player { visual, shared, sound: Sound::new() });
        let (for_thread, control_there) = (window.clone(), control.clone());
        std::thread::Builder::new()
            .name("popamp-visualizer".into())
            .spawn(move || {
                if let Err(why) = popamp_visual::run(for_thread, feed, control_there) {
                    events.send(AppEvent::VisualizerStopped(id, why));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(VisualizerWindow { window, control, full })
    }

    /// The thread is told to stop and not waited for: a stuck GPU must not stop the player.
    pub fn close(&self) {
        self.control.stop.store(true, Ordering::Relaxed);
    }

    /// False when the window is to be closed.
    pub fn event(&self, ev: WindowEvent) -> bool {
        match ev {
            WindowEvent::CloseRequested => return false,
            WindowEvent::Resized(size) => *self.control.size.lock() = pixels(size),
            WindowEvent::KeyboardInput { event, .. } => return self.key(&event),
            _ => {}
        }
        true
    }

    /// F toggles full screen; Esc leaves it, or closes the window when not in it.
    fn key(&self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed || event.repeat {
            return true;
        }
        let full = self.window.fullscreen().is_some();
        match event.physical_key {
            PhysicalKey::Code(KeyCode::KeyF) => self.set_full(!full),
            PhysicalKey::Code(KeyCode::Escape) if full => self.set_full(false),
            PhysicalKey::Code(KeyCode::Escape) => return false,
            _ => {}
        }
        true
    }

    fn set_full(&self, on: bool) {
        self.window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        self.full.store(on, Ordering::Relaxed);
    }
}

/// Full screen with music playing is being watched; a paused player left
/// in full screen may still let the monitor sleep.
fn keep_screen_on_while_watched(control: &Arc<Control>, full: &Arc<AtomicBool>, shared: Arc<Shared>) {
    let (control, full) = (control.clone(), full.clone());
    crate::keep_awake::spawn(
        move || full.load(Ordering::Relaxed) && shared.state() == PlayState::Playing,
        move || control.stop.load(Ordering::Relaxed),
    );
}

fn pixels(size: PhysicalSize<u32>) -> (u32, u32) {
    (size.width, size.height)
}

struct Player {
    visual: Visual,
    shared: Arc<Shared>,
    sound: Sound,
}

impl Feed for Player {
    fn scene(&mut self) -> Result<Vec<u8>, String> {
        self.visual.scene()
    }

    fn data(&mut self, frame: &[u8]) -> Result<Vec<u8>, String> {
        self.visual.frame(frame)
    }

    fn sound(&mut self, wave: &mut [f32; WAVE_LEN], spectrum: &mut [f32; SPECTRUM_LEN]) {
        if self.shared.state() == PlayState::Playing {
            let played = self.shared.played_frames();
            self.shared.vis.lock().read(played, wave);
        } else {
            wave.fill(0.0);
        }
        self.sound.spectrum(wave, spectrum);
    }
}

/// Turns the last samples into levels from 0 (silence) to 1 (full scale).
struct Sound {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
}

impl Sound {
    fn new() -> Sound {
        let window = (0..WAVE_LEN).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WAVE_LEN as f32).cos()).collect();
        Sound { fft: FftPlanner::new().plan_fft_forward(WAVE_LEN), window, scratch: vec![Complex::default(); WAVE_LEN] }
    }

    fn spectrum(&mut self, wave: &[f32; WAVE_LEN], out: &mut [f32; SPECTRUM_LEN]) {
        for ((c, s), w) in self.scratch.iter_mut().zip(wave).zip(&self.window) {
            *c = Complex::new(s * w, 0.0);
        }
        self.fft.process(&mut self.scratch);
        // A Hann window halves a full-scale sine's peak, which sits at a quarter of the length.
        let full_scale = WAVE_LEN as f32 / 4.0;
        for (o, c) in out.iter_mut().zip(&self.scratch) {
            let db = 20.0 * (c.norm() / full_scale + 1e-9).log10();
            *o = (1.0 - db / FLOOR_DB).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_scale_tone_peaks_at_its_bin_and_silence_is_zero() {
        const BIN: usize = 40;
        let mut wave = [0.0; WAVE_LEN];
        for (i, s) in wave.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * BIN as f32 * i as f32 / WAVE_LEN as f32).sin();
        }
        let mut sound = Sound::new();
        let mut levels = [0.0; SPECTRUM_LEN];
        sound.spectrum(&wave, &mut levels);
        assert!((levels[BIN] - 1.0).abs() < 0.01, "{}", levels[BIN]);
        assert!(levels[BIN + 10] < 0.2, "{}", levels[BIN + 10]);
        sound.spectrum(&[0.0; WAVE_LEN], &mut levels);
        assert!(levels.iter().all(|l| *l == 0.0));
    }
}
