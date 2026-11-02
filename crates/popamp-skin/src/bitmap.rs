//! A plain `0x00RRGGBB` pixel buffer with the few drawing primitives the
//! default skin generator and the renderer need.

#[derive(Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl std::fmt::Debug for Bitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bitmap({}x{})", self.w, self.h)
    }
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Linear blend of two colours, `t` in 0..=1.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |shift: u32| {
        let x = ((a >> shift) & 0xff) as f32;
        let y = ((b >> shift) & 0xff) as f32;
        ((x + (y - x) * t).round() as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
}

/// Pixels with transparency: premultiplied `0xAARRGGBB`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Rgba {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

/// Premultiplied `p` over opaque `dst`.
fn over(p: u32, dst: u32) -> u32 {
    let keep = 255 - (p >> 24);
    let ch = |shift: u32| ((p >> shift) & 0xff) + (((dst >> shift) & 0xff) * keep + 127) / 255;
    (ch(16).min(255) << 16) | (ch(8).min(255) << 8) | ch(0).min(255)
}

impl Bitmap {
    /// Lays `src` over the bitmap with its top-left corner at `(x, y)`,
    /// touching only pixels inside the `clip` rectangle (x, y, w, h).
    pub fn overlay(&mut self, src: &Rgba, x: i32, y: i32, clip: (i32, i32, i32, i32)) {
        let (cx, cy, cw, ch) = clip;
        let (x0, x1) = (x.max(cx).max(0), (x + src.w).min(cx + cw).min(self.w));
        let (y0, y1) = (y.max(cy).max(0), (y + src.h).min(cy + ch).min(self.h));
        for yy in y0..y1 {
            for xx in x0..x1 {
                let p = src.px[((yy - y) * src.w + xx - x) as usize];
                let dst = &mut self.px[(yy * self.w + xx) as usize];
                *dst = over(p, *dst);
            }
        }
    }

    pub fn new(w: i32, h: i32, color: u32) -> Self {
        Self { w, h, px: vec![color; (w.max(0) * h.max(0)) as usize] }
    }

    /// Decodes BMP/PNG/GIF/JPEG (format sniffed from the bytes).
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.to_rgb8();
        let (w, h) = img.dimensions();
        let px = img.pixels().map(|p| rgb(p[0], p[1], p[2])).collect();
        Ok(Self { w: w as i32, h: h as i32, px })
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Option<u32> {
        (x >= 0 && y >= 0 && x < self.w && y < self.h).then(|| self.px[(y * self.w + x) as usize])
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.px[(y * self.w + x) as usize] = c;
        }
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.px[(yy * self.w + xx) as usize] = c;
            }
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: u32) {
        self.fill(x, y, w, 1, c);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, c: u32) {
        self.fill(x, y, 1, h, c);
    }

    /// One-pixel bevel: `light` on top/left, `dark` on bottom/right.
    pub fn bevel(&mut self, x: i32, y: i32, w: i32, h: i32, light: u32, dark: u32) {
        self.hline(x, y, w, light);
        self.vline(x, y, h, light);
        self.hline(x, y + h - 1, w, dark);
        self.vline(x + w - 1, y, h, dark);
    }

    /// Copies a rectangle from `src` (clipped on both sides).
    #[allow(clippy::too_many_arguments)]
    pub fn blit(&mut self, src: &Bitmap, sx: i32, sy: i32, w: i32, h: i32, dx: i32, dy: i32) {
        for row in 0..h {
            let (syy, dyy) = (sy + row, dy + row);
            if syy < 0 || syy >= src.h || dyy < 0 || dyy >= self.h {
                continue;
            }
            for col in 0..w {
                let (sxx, dxx) = (sx + col, dx + col);
                if sxx < 0 || sxx >= src.w || dxx < 0 || dxx >= self.w {
                    continue;
                }
                self.px[(dyy * self.w + dxx) as usize] = src.px[(syy * src.w + sxx) as usize];
            }
        }
    }

    pub fn to_rgb_bytes(&self) -> Vec<u8> {
        self.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect()
    }

    /// Saves as PNG (used by golden-image tests and `--render-skin`).
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        image::save_buffer(path, &self.to_rgb_bytes(), self.w as u32, self.h as u32, image::ColorType::Rgb8).map_err(|e| e.to_string())
    }
}
