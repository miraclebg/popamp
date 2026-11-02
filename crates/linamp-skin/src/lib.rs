//! Classic skin support: `.wsz`/`.zip` archives or unpacked
//! directories, with per-file fallback to Linamp's own default skin.

pub mod bitmap;
mod classic;
pub mod cursors;
pub mod default_skin;
pub mod layout;
mod names;
pub mod sprites;
pub mod svg;
pub mod textcfg;
pub mod vector;

pub use bitmap::{Bitmap, Rgba};
pub use cursors::{CursorImage, CursorKind};
pub use layout::{Layout, Picture};
use layout::{Shape, WindowKey};
pub use sprites::{Rect, Sheet};
pub use textcfg::{PlColors, Regions, TextStyle};

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
    /// The skin's own mouse cursors; the system cursor stands in for the rest.
    pub cursors: HashMap<CursorKind, CursorImage>,
    /// The windows and their elements.
    pub layout: Layout,
    /// A vector skin's pictures.
    pub svgs: svg::Svgs,
    /// Drawn at screen size rather than scaled up from x1.
    pub vector: bool,
    /// A vector skin's drawings of classic sheets, used in their place.
    pub sheet_svgs: HashMap<Sheet, u16>,
    /// A vector skin's small texts (title, kbps...); classic skins use
    /// `text.bmp`.
    pub text: Option<TextStyle>,
    /// A vector skin's play time; classic skins use their digit bitmaps.
    pub clock: Option<TextStyle>,
    /// A vector skin's playlist font file; otherwise `pl.font` names a
    /// system font.
    pub pl_font: Option<Vec<u8>>,
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
            cursors: HashMap::new(),
            layout: Layout::default(),
            svgs: svg::Svgs::default(),
            vector: false,
            sheet_svgs: HashMap::new(),
            text: None,
            clock: None,
            pl_font: None,
        }
        .with_layout(Regions::default())
    }

    /// Builds the classic layout for the skin's bitmaps and window shapes.
    pub(crate) fn with_layout(mut self, regions: Regions) -> Skin {
        self.layout = classic::layout(self.sheet(Sheet::Volume).h, self.sheet(Sheet::Balance).h);
        let shapes = [(WindowKey::Main, regions.normal, regions.window_shade), (WindowKey::Eq, regions.equalizer, regions.equalizer_ws)];
        for (key, normal, shade) in shapes {
            let Some(w) = self.layout.windows.iter_mut().find(|w| w.key == key) else { continue };
            w.normal.shape = normal.map(Shape::Polygons);
            if let Some(s) = &mut w.shade {
                s.shape = shade.map(Shape::Polygons);
            }
        }
        self
    }

    pub fn sheet(&self, s: Sheet) -> &Bitmap {
        &self.sheets[&s]
    }

    /// Loads a skin archive or directory: a vector skin when it has a
    /// `skin.toml`, a classic one otherwise.
    pub fn load(path: &Path) -> Result<Skin, String> {
        let read = |key: Key| if path.is_dir() { read_dir_files(path, key) } else { read_zip_files(path, key) };
        if is_vector(path) {
            return read(vector_key).and_then(|files| vector::load(&files)).map_err(|e| format!("{}: {e}", path.display()));
        }
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self::from_files(name, &read(classic_key)?))
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
        let regions = files.get("region.txt").map(|t| textcfg::parse_region(&linamp_core::text::decode_lossy(t))).unwrap_or_default();
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
        skin.with_layout(regions)
    }
}

const MAX_FILE: u64 = 16 << 20;

fn wanted(name: &str) -> bool {
    name.ends_with(".bmp") || name.ends_with(".png") || name.ends_with(".txt") || name.ends_with(".cur")
}

fn is_vector(path: &Path) -> bool {
    if path.is_dir() {
        return path.join(vector::FILE).is_file();
    }
    let zip = std::fs::File::open(path).ok().and_then(|f| zip::ZipArchive::new(f).ok());
    zip.is_some_and(|mut z| z.by_name(vector::FILE).is_ok())
}

/// The name a file inside a skin is known by, or `None` to skip it.
type Key = fn(&str) -> Option<String>;

/// Classic skins: the lower-case base name, wherever the file is.
fn classic_key(path: &str) -> Option<String> {
    let base = path.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
    wanted(&base).then_some(base)
}

/// Vector skins: the path from the skin's top folder.
fn vector_key(path: &str) -> Option<String> {
    Some(path.replace('\\', "/"))
}

fn read_zip_files(path: &Path, key: Key) -> Result<HashMap<String, Vec<u8>>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    read_zip(file, key).map_err(|e| format!("{}: {e}", path.display()))
}

/// Checks downloaded bytes before they are saved as a skin: a zip archive
/// whose main window bitmap decodes.
pub fn check_archive(bytes: &[u8]) -> Result<(), String> {
    let files = read_zip(std::io::Cursor::new(bytes), classic_key)?;
    let main = files.get(Sheet::Main.file_name()).ok_or("no main window bitmap in the archive")?;
    Bitmap::decode(main).map(drop)
}

fn read_zip(file: impl Read + std::io::Seek, key: Key) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("not a skin archive: {e}"))?;
    let mut out = HashMap::new();
    for i in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(i) else { continue };
        if entry.is_dir() || entry.size() > MAX_FILE {
            continue;
        }
        let Some(base) = key(entry.name()).filter(|k| !out.contains_key(k)) else { continue };
        let mut data = Vec::with_capacity(entry.size() as usize);
        if entry.by_ref().take(MAX_FILE).read_to_end(&mut data).is_ok() {
            out.insert(base, data);
        }
    }
    Ok(out)
}

fn read_dir_files(dir: &Path, key: Key) -> Result<HashMap<String, Vec<u8>>, String> {
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
            let inside = p.strip_prefix(dir).unwrap_or(&p).to_string_lossy();
            let Some(base) = key(&inside).filter(|k| !out.contains_key(k)) else { continue };
            if entry.metadata().map(|m| m.len() <= MAX_FILE).unwrap_or(false) {
                if let Ok(data) = std::fs::read(&p) {
                    out.insert(base, data);
                }
            }
        }
    }
    Ok(out)
}

/// When a skin last changed: the newest modification time of the archive,
/// or of the folder and anything in it.
pub fn changed_at(path: &Path) -> Option<std::time::SystemTime> {
    let mut newest = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let mut stack = vec![(path.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            newest = newest.max(meta.modified().unwrap_or(newest));
            if meta.is_dir() && depth < 3 {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    Some(newest)
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

    #[test]
    fn regions_shape_the_main_and_eq_windows() {
        let region = b"[Normal]\nNumPoints=4\nPointList=0,0, 275,0, 275,116, 0,116\n[WindowShade]\nNumPoints=3\nPointList=0,0, 9,0, 0,9\n";
        let skin = Skin::from_files("t".into(), &HashMap::from([("region.txt".to_string(), region.to_vec())]));
        let main = skin.layout.window(WindowKey::Main).unwrap();
        assert_eq!(main.normal.shape, Some(Shape::Polygons(vec![vec![(0, 0), (275, 0), (275, 116), (0, 116)]])));
        assert_eq!(main.mode(true).shape, Some(Shape::Polygons(vec![vec![(0, 0), (9, 0), (0, 9)]])));
        assert_eq!(skin.layout.window(WindowKey::Eq).unwrap().normal.shape, None);
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
