//! Equalizer, limiter and gain stages.

pub const WINAMP_BANDS: [f32; 10] = [60.0, 170.0, 310.0, 600.0, 1000.0, 3000.0, 6000.0, 12000.0, 14000.0, 16000.0];
pub const ISO_BANDS: [f32; 10] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqSettings {
    pub enabled: bool,
    pub iso: bool,
    pub limiter: bool,
    /// Band gains in dB, ±12.
    pub bands_db: [f32; 10],
    pub preamp_db: f32,
}

impl Default for EqSettings {
    fn default() -> Self {
        Self { enabled: false, iso: false, limiter: true, bands_db: [0.0; 10], preamp_db: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Coeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    active: bool,
}

impl Coeffs {
    /// RBJ cookbook peaking filter.
    fn peaking(freq: f32, gain_db: f32, q: f32, rate: f32) -> Self {
        if gain_db.abs() < 0.05 || freq >= rate * 0.45 {
            return Self::default();
        }
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
            active: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

pub struct Equalizer {
    settings: EqSettings,
    rate: u32,
    coeffs: [Coeffs; 10],
    state: [[State; 10]; 2],
    preamp: f32,
    limiter: Limiter,
}

impl Equalizer {
    pub fn new(rate: u32) -> Self {
        let mut eq = Self {
            settings: EqSettings::default(),
            rate: rate.max(1),
            coeffs: Default::default(),
            state: Default::default(),
            preamp: 1.0,
            limiter: Limiter::new(rate),
        };
        eq.recompute();
        eq
    }

    pub fn settings(&self) -> &EqSettings {
        &self.settings
    }

    pub fn set(&mut self, settings: EqSettings) {
        if settings != self.settings {
            self.settings = settings;
            self.recompute();
        }
    }

    pub fn set_rate(&mut self, rate: u32) {
        if rate != self.rate && rate > 0 {
            self.rate = rate;
            self.limiter = Limiter::new(rate);
            self.state = Default::default();
            self.recompute();
        }
    }

    pub fn reset(&mut self) {
        self.state = Default::default();
    }

    fn recompute(&mut self) {
        let freqs = if self.settings.iso { ISO_BANDS } else { WINAMP_BANDS };
        for (i, c) in self.coeffs.iter_mut().enumerate() {
            *c = Coeffs::peaking(freqs[i], self.settings.bands_db[i].clamp(-12.0, 12.0), 1.1, self.rate as f32);
        }
        self.preamp = 10f32.powf(self.settings.preamp_db.clamp(-12.0, 12.0) / 20.0);
    }

    /// Processes interleaved stereo in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        if !self.settings.enabled {
            return;
        }
        for frame in buf.as_chunks_mut::<2>().0 {
            for (ch, sample) in frame.iter_mut().enumerate() {
                let mut x = *sample * self.preamp;
                for (c, s) in self.coeffs.iter().zip(self.state[ch].iter_mut()) {
                    if !c.active {
                        continue;
                    }
                    let y = c.b0 * x + c.b1 * s.x1 + c.b2 * s.x2 - c.a1 * s.y1 - c.a2 * s.y2;
                    s.x2 = s.x1;
                    s.x1 = x;
                    s.y2 = s.y1;
                    s.y1 = y;
                    x = y;
                }
                *sample = x;
            }
        }
        // Flush denormals that build up in silence.
        for st in self.state.iter_mut().flatten() {
            for v in [&mut st.x1, &mut st.x2, &mut st.y1, &mut st.y2] {
                if v.abs() < 1e-20 {
                    *v = 0.0;
                }
            }
        }
        if self.settings.limiter {
            self.limiter.process(buf);
        }
    }
}

/// Peak limiter with instant attack and ~80 ms release. Keeps boosted EQ
/// settings from clipping.
pub struct Limiter {
    gain: f32,
    release: f32,
    threshold: f32,
}

impl Limiter {
    pub fn new(rate: u32) -> Self {
        let release_s = 0.08;
        Self { gain: 1.0, release: 1.0 - (-1.0 / (release_s * rate.max(1) as f32)).exp(), threshold: 0.966 }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for frame in buf.as_chunks_mut::<2>().0 {
            let peak = frame[0].abs().max(frame[1].abs());
            let target = if peak * self.gain > self.threshold { self.threshold / peak } else { 1.0 };
            if target < self.gain {
                self.gain = target;
            } else {
                self.gain += (target.min(1.0) - self.gain) * self.release;
            }
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }
}

/// Maps Winamp's 0..=255 volume to linear gain. A squared curve gives a
/// roughly perceptual response (half way ≈ -12 dB).
pub fn volume_gain(volume: u8) -> f32 {
    let v = volume as f32 / 255.0;
    v * v
}

/// Per-channel gains for balance -127..=127.
pub fn balance_gains(balance: i8) -> (f32, f32) {
    let b = balance.clamp(-127, 127) as f32 / 127.0;
    if b >= 0.0 {
        (1.0 - b, 1.0)
    } else {
        (1.0, 1.0 + b)
    }
}

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_gain(eq: &mut Equalizer, freq: f32, rate: u32) -> f32 {
        let n = rate as usize; // one second
        let mut buf: Vec<f32> = (0..n)
            .flat_map(|i| {
                let s = (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin() * 0.1;
                [s, s]
            })
            .collect();
        eq.reset();
        eq.process(&mut buf);
        // Measure RMS over the second half (after the filter settles).
        let tail = &buf[n..];
        let rms = (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt();
        20.0 * (rms / (0.1 / 2f32.sqrt())).log10()
    }

    #[test]
    fn band_gain_hits_target() {
        let rate = 44_100;
        for (iso, freqs) in [(false, WINAMP_BANDS), (true, ISO_BANDS)] {
            for band in [0usize, 4, 9] {
                let mut s = EqSettings { enabled: true, iso, limiter: false, ..Default::default() };
                s.bands_db[band] = 12.0;
                let mut eq = Equalizer::new(rate);
                eq.set(s);
                let g = sine_gain(&mut eq, freqs[band], rate);
                // Neighbouring bands overlap a little, so allow +0..+1.5 dB slack.
                assert!((11.5..13.5).contains(&g), "band {band} iso={iso}: {g} dB");
            }
        }
    }

    #[test]
    fn flat_is_transparent_and_disabled_is_bypass() {
        let mut eq = Equalizer::new(48_000);
        eq.set(EqSettings { enabled: true, limiter: false, ..Default::default() });
        let g = sine_gain(&mut eq, 1000.0, 48_000);
        assert!(g.abs() < 0.05, "{g}");
        let mut buf = vec![0.5, -0.5];
        let mut off = Equalizer::new(48_000);
        off.set(EqSettings { enabled: false, preamp_db: 12.0, ..Default::default() });
        off.process(&mut buf);
        assert_eq!(buf, [0.5, -0.5]);
    }

    #[test]
    fn limiter_prevents_clipping() {
        let mut l = Limiter::new(44_100);
        let mut buf: Vec<f32> = (0..4410).flat_map(|i| [(i as f32 * 0.1).sin() * 2.0; 2]).collect();
        l.process(&mut buf);
        assert!(buf.iter().all(|s| s.abs() <= 0.967), "max {}", buf.iter().fold(0f32, |a, b| a.max(b.abs())));
    }

    #[test]
    fn gains() {
        assert_eq!(volume_gain(0), 0.0);
        assert_eq!(volume_gain(255), 1.0);
        assert_eq!(balance_gains(0), (1.0, 1.0));
        assert_eq!(balance_gains(127), (0.0, 1.0));
        assert_eq!(balance_gains(-127), (1.0, 0.0));
    }
}
