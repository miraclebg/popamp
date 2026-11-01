//! Classic skin support: `.wsz`/`.zip` archives or unpacked
//! directories, with per-file fallback to Linamp's own default skin.

pub mod bitmap;
pub mod cursors;
pub mod default_skin;
pub mod sprites;
pub mod textcfg;

pub use bitmap::Bitmap;
pub use cursors::{CursorImage, CursorKind};
pub use sprites::{Rect, Sheet};
pub use textcfg::{PlColors, Regions};

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

pub struct Skin {
    pub name: String,
    sheets: HashMap<Sheet, Bitmap>,
    /// The skin provides `nums_ex.bmp` (which has its own minus sign).
    pub has_nums_ex: bool,
    pub viscolor: [u32; 24],
    pub pl: PlColors,
    pub regions: Regions,
    /// The skin's own mouse cursors; the system cursor stands in for the rest.
    pub cursors: HashMap<CursorKind, CursorImage>,
}

impl Skin {
    pub fn builtin() -> Skin {
        let sheets = Sheet::ALL.iter().map(|&s| (s, default_skin::sheet(s))).collect();
        Skin {
            name: "Linamp Default".into(),
            sheets,
            has_nums_ex: false,
            viscolor: default_skin::viscolors(),
            pl: default_skin::pl_colors(),
            regions: Regions::default(),
            cursors: HashMap::new(),
        }
    }

    pub fn sheet(&self, s: Sheet) -> &Bitmap {
        &self.sheets[&s]
    }

    /// Loads a skin archive or directory.
    pub fn load(path: &Path) -> Result<Skin, String> {
        let files = if path.is_dir() { read_dir_files(path)? } else { read_zip_files(path)? };
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self::from_files(name, &files))
    }

    /// Builds a skin from lower-cased basename → bytes.
    pub fn from_files(name: String, files: &HashMap<String, Vec<u8>>) -> Skin {
        let mut skin = Skin::builtin();
        skin.name = name;
        let decode = |file: &str| -> Option<Bitmap> {
            let bytes = files.get(file)?;
            match Bitmap::decode(bytes) {
                Ok(b) if b.w > 0 && b.h > 0 => Some(b),
                Ok(_) => None,
                Err(e) => {
                    log::warn!("skin {}: cannot decode {file}: {e}", skin.name);
                    None
                }
            }
        };
        let mut found = 0;
        for sheet in Sheet::ALL {
            if let Some(b) = decode(sheet.file_name()) {
                skin.sheets.insert(sheet, b);
                found += 1;
                if sheet == Sheet::NumsEx {
                    skin.has_nums_ex = true;
                }
            }
        }
        // Old skins without balance.bmp reuse their volume bitmap.
        if !files.contains_key("balance.bmp") || decode("balance.bmp").is_none() {
            if let Some(v) = decode("volume.bmp") {
                skin.sheets.insert(Sheet::Balance, v);
            }
        }
        if let Some(t) = files.get("viscolor.txt") {
            skin.viscolor = textcfg::parse_viscolor(&linamp_core::text::decode_lossy(t), &skin.viscolor);
        }
        if let Some(t) = files.get("pledit.txt") {
            // Skins that only ship pledit.txt start from the classic colours.
            skin.pl = textcfg::parse_pledit(&linamp_core::text::decode_lossy(t), &PlColors::default());
        } else if found > 0 {
            skin.pl = PlColors::default();
        }
        if let Some(t) = files.get("region.txt") {
            skin.regions = textcfg::parse_region(&linamp_core::text::decode_lossy(t));
        }
        for kind in CursorKind::ALL {
            let Some(bytes) = files.get(kind.file_name()) else { continue };
            match cursors::parse(bytes) {
                Some(c) => {
                    skin.cursors.insert(kind, c);
                }
                None => log::warn!("skin {}: cannot decode {}", skin.name, kind.file_name()),
            }
        }
        if found == 0 {
            log::warn!("skin {}: no recognised bitmaps, using defaults", skin.name);
        }
        skin
    }
}

const MAX_FILE: u64 = 16 << 20;

fn wanted(name: &str) -> bool {
    name.ends_with(".bmp") || name.ends_with(".png") || name.ends_with(".txt") || name.ends_with(".cur")
}

fn read_zip_files(path: &Path) -> Result<HashMap<String, Vec<u8>>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read_zip(file).map_err(|e| format!("{}: {e}", path.display()))
}

/// Checks downloaded bytes before they are saved as a skin: a zip archive
/// whose main window bitmap decodes.
pub fn check_archive(bytes: &[u8]) -> Result<(), String> {
    let files = read_zip(std::io::Cursor::new(bytes))?;
    let main = files.get(Sheet::Main.file_name()).ok_or("no main window bitmap in the archive")?;
    Bitmap::decode(main).map(drop)
}

fn read_zip(file: impl Read + std::io::Seek) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a skin archive: {e}"))?;
    let mut out = HashMap::new();
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else { continue };
        if entry.is_dir() || entry.size() > MAX_FILE {
            continue;
        }
        let base = entry.name().rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
        if !wanted(&base) || out.contains_key(&base) {
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        if entry.by_ref().take(MAX_FILE).read_to_end(&mut data).is_ok() {
            out.insert(base, data);
        }
    }
    Ok(out)
}

fn read_dir_files(dir: &Path) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut out = HashMap::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if depth < 2 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            let base = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if wanted(&base) && !out.contains_key(&base) && entry.metadata().map(|m| m.len() <= MAX_FILE).unwrap_or(false) {
                if let Ok(data) = std::fs::read(&p) {
                    out.insert(base, data);
                }
            }
        }
    }
    Ok(out)
}

/// Lists installable skins in `dir` (archives and sub-directories).
pub fn list_skins(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                || p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("wsz") || e.eq_ignore_ascii_case("zip"))
        })
        .collect();
    v.sort_by(|a, b| linamp_core::text::natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder as _;

    #[test]
    fn loads_skin_cursors() {
        let mut files = HashMap::new();
        files.insert("normal.cur".to_string(), cursors::tests::sample_cur());
        files.insert("close.cur".to_string(), b"broken".to_vec());
        let skin = Skin::from_files("t".into(), &files);
        assert_eq!(skin.cursors.keys().collect::<Vec<_>>(), [&CursorKind::Normal]);
        assert!(Skin::builtin().cursors.is_empty());
    }

    #[test]
    fn builtin_sheets_have_reference_sizes() {
        let skin = Skin::builtin();
        for s in Sheet::ALL {
            assert_eq!((skin.sheet(s).w, skin.sheet(s).h), s.size(), "{s:?}");
        }
    }

    #[test]
    fn loads_from_files_with_fallbacks() {
        let mut files = HashMap::new();
        let mut img = Vec::new();
        image::codecs::png::PngEncoder::new(&mut img).write_image(&[255u8; 68 * 433 * 3], 68, 433, image::ExtendedColorType::Rgb8).unwrap();
        files.insert("volume.bmp".to_string(), img); // PNG data despite the name
        files.insert("main.bmp".to_string(), b"not an image".to_vec());
        files.insert("pledit.txt".to_string(), b"[Text]\nNormal=#112233\n".to_vec());
        let skin = Skin::from_files("t".into(), &files);
        assert_eq!(skin.sheet(Sheet::Volume).px[0], 0xffffff);
        assert_eq!(skin.sheet(Sheet::Balance).px[0], 0xffffff, "balance falls back to volume");
        assert_eq!(skin.sheet(Sheet::Main).w, 275, "bad main.bmp falls back to default");
        assert_eq!(skin.pl.normal, 0x112233);
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn checks_downloaded_archives() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(&[0u8; 3], 1, 1, image::ExtendedColorType::Rgb8).unwrap();
        assert_eq!(check_archive(&zip_of(&[("Skin/MAIN.BMP", &png)])), Ok(()));
        assert!(check_archive(b"<html>not found</html>").unwrap_err().contains("not a skin archive"));
        assert!(check_archive(&zip_of(&[("eqmain.bmp", &png)])).unwrap_err().contains("main window"));
        assert!(check_archive(&zip_of(&[("main.bmp", b"junk")])).is_err());
    }
}
