//! Winamp EQ preset files: single `.eqf` presets and `.q1` libraries share
//! one layout.
//!
//! ```text
//! "Winamp EQ library file v1.1" 0x1A "!--"
//! repeat { name[257] (NUL padded), bands[10] (0..=63, 0 = +12 dB), preamp }
//! ```

use crate::text::decode_lossy;
use std::io;
use std::path::Path;

pub const HEADER: &[u8; 31] = b"Winamp EQ library file v1.1\x1a!--";
const NAME_LEN: usize = 257;
const RECORD_LEN: usize = NAME_LEN + 11;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    /// 0..=63, 0 = +12 dB, 63 = -12 dB.
    pub bands: [u8; 10],
    pub preamp: u8,
}

impl Preset {
    pub fn flat(name: &str) -> Self {
        Self { name: name.into(), bands: [31; 10], preamp: 31 }
    }
}

/// Converts the 0..=63 scale to dB (0 → +12, 63 → -12).
pub fn value_to_db(v: u8) -> f32 {
    12.0 - (v.min(63) as f32) * 24.0 / 63.0
}

pub fn db_to_value(db: f32) -> u8 {
    // Bias down so 0 dB lands on 31, Winamp's flat position.
    ((12.0 - db.clamp(-12.0, 12.0)) * 63.0 / 24.0 - 1e-3).round().max(0.0) as u8
}

pub fn parse(data: &[u8]) -> io::Result<Vec<Preset>> {
    let body = data.strip_prefix(&HEADER[..]).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a Winamp EQ file"))?;
    Ok(body
        .as_chunks::<RECORD_LEN>()
        .0
        .iter()
        .map(|rec| {
            let name_bytes = &rec[..NAME_LEN];
            let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
            let mut bands = [0u8; 10];
            for (b, &v) in bands.iter_mut().zip(&rec[NAME_LEN..NAME_LEN + 10]) {
                *b = v.min(63);
            }
            Preset { name: decode_lossy(&name_bytes[..end]), bands, preamp: rec[NAME_LEN + 10].min(63) }
        })
        .collect())
}

pub fn serialize(presets: &[Preset]) -> Vec<u8> {
    let mut out = HEADER.to_vec();
    for p in presets {
        let mut name = [0u8; NAME_LEN];
        let bytes = p.name.as_bytes();
        let n = bytes.len().min(NAME_LEN - 1);
        name[..n].copy_from_slice(&bytes[..n]);
        out.extend_from_slice(&name);
        out.extend(p.bands.iter().map(|b| (*b).min(63)));
        out.push(p.preamp.min(63));
    }
    out
}

pub fn read_file(path: &Path) -> io::Result<Vec<Preset>> {
    parse(&std::fs::read(path)?)
}

pub fn write_file(path: &Path, presets: &[Preset]) -> io::Result<()> {
    crate::config::write_atomic(path, &serialize(presets))
}

/// Built-in presets. Values are this project's own, not copied from Winamp.
pub fn builtin_presets() -> Vec<Preset> {
    let p = |name: &str, db: [f32; 10]| Preset { name: name.into(), bands: db.map(db_to_value), preamp: 31 };
    vec![
        p("Flat", [0.0; 10]),
        p("Bass Boost", [7.0, 6.0, 4.5, 2.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0]),
        p("Bass & Treble", [6.0, 4.5, 2.0, 0.0, -1.5, -1.0, 1.5, 4.0, 5.5, 6.0]),
        p("Treble Boost", [0.0, 0.0, 0.0, 0.0, 0.5, 2.0, 4.0, 6.0, 7.0, 7.5]),
        p("Classical", [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -3.5, -3.5, -3.5, -5.0]),
        p("Club", [0.0, 0.0, 3.0, 4.0, 4.0, 4.0, 2.5, 0.0, 0.0, 0.0]),
        p("Dance", [7.5, 5.5, 2.0, 0.0, 0.0, -3.5, -4.0, -4.0, 0.0, 0.0]),
        p("Headphones", [3.5, 7.0, 3.5, -2.5, -1.5, 1.0, 3.0, 6.0, 7.5, 8.5]),
        p("Large Hall", [7.5, 7.5, 4.0, 4.0, 0.0, -3.0, -3.0, -3.0, 0.0, 0.0]),
        p("Live", [-3.0, 0.0, 2.5, 3.0, 3.5, 3.5, 2.5, 1.5, 1.5, 1.5]),
        p("Party", [5.5, 5.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 5.5, 5.5]),
        p("Pop", [-1.0, 3.0, 5.0, 5.5, 3.5, -0.5, -1.5, -1.5, -1.0, -1.0]),
        p("Reggae", [0.0, 0.0, -0.5, -4.0, 0.0, 4.5, 4.5, 0.0, 0.0, 0.0]),
        p("Rock", [5.0, 3.0, -3.5, -5.0, -2.0, 2.5, 5.5, 7.0, 7.0, 7.0]),
        p("Ska", [-1.5, -3.0, -2.5, -0.5, 2.5, 3.5, 5.5, 6.0, 7.0, 6.0]),
        p("Soft", [3.0, 1.0, -0.5, -1.5, -0.5, 2.5, 5.0, 6.0, 7.0, 7.5]),
        p("Soft Rock", [2.5, 2.5, 1.5, -0.5, -2.5, -3.0, -2.0, -0.5, 1.5, 5.0]),
        p("Techno", [5.0, 3.5, 0.0, -3.5, -3.0, 0.0, 5.0, 6.0, 6.0, 5.5]),
        p("Vocal", [-2.0, -3.0, -3.0, 1.5, 4.0, 4.0, 3.0, 1.5, 0.0, -2.0]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_byte_exact() {
        let presets = vec![Preset { name: "Rock".into(), bands: [0, 10, 20, 31, 40, 50, 63, 1, 2, 3], preamp: 12 }, Preset::flat("Flat")];
        let bytes = serialize(&presets);
        assert_eq!(bytes.len(), 31 + 2 * RECORD_LEN);
        assert_eq!(&bytes[..31], HEADER);
        let back = parse(&bytes).unwrap();
        assert_eq!(back, presets);
        assert_eq!(serialize(&back), bytes);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"hello").is_err());
    }

    #[test]
    fn db_scale() {
        assert_eq!(value_to_db(0), 12.0);
        assert_eq!(value_to_db(63), -12.0);
        assert_eq!(db_to_value(0.0), 31);
        assert_eq!(db_to_value(12.0), 0);
        assert_eq!(db_to_value(-30.0), 63);
        for v in 0..=63u8 {
            assert_eq!(db_to_value(value_to_db(v)), v);
        }
    }
}
