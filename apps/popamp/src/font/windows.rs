//! System fonts on Windows: the font list in the registry, and the fonts
//! Windows itself falls back to for scripts the chosen one lacks.

use std::path::{Path, PathBuf};

/// Families with the file each has on Windows, for when the registry
/// does not list them (Wine keeps its own fonts out of it).
type Known = (&'static str, &'static [&'static str]);

const SANS: &[Known] =
    &[("Segoe UI", &["segoeui.ttf"]), ("Tahoma", &["tahoma.ttf"]), ("Arial", &["arial.ttf"]), ("Microsoft Sans Serif", &["micross.ttf"])];
const SERIF: &[Known] = &[("Times New Roman", &["times.ttf"]), ("Georgia", &["georgia.ttf"])];
const MONO: &[Known] = &[("Consolas", &["consola.ttf"]), ("Courier New", &["cour.ttf"]), ("Lucida Console", &["lucon.ttf"])];
/// After the sans-serif ones, by script.
const FALLBACK: &[Known] = &[
    ("Segoe UI Symbol", &["seguisym.ttf"]),
    ("Segoe UI Emoji", &["seguiemj.ttf"]),
    ("Segoe UI Historic", &["seguihis.ttf"]),
    ("Microsoft YaHei", &["msyh.ttc", "msyh.ttf"]),
    ("Microsoft JhengHei", &["msjh.ttc"]),
    ("Yu Gothic", &["YuGothM.ttc"]),
    ("MS Gothic", &["msgothic.ttc"]),
    ("Malgun Gothic", &["malgun.ttf"]),
    ("SimSun", &["simsun.ttc"]),
    ("Nirmala UI", &["Nirmala.ttc", "nirmala.ttf"]),
    ("Leelawadee UI", &["LeelawUI.ttf"]),
    ("Ebrima", &["ebrima.ttf"]),
    ("Gadugi", &["gadugi.ttf"]),
    ("Sylfaen", &["sylfaen.ttf"]),
];

/// The families a generic CSS name stands for; `None` for a real family.
fn generic(name: &str) -> Option<&'static [Known]> {
    match name.to_ascii_lowercase().as_str() {
        "sans-serif" | "sans" => Some(SANS),
        "serif" => Some(SERIF),
        "monospace" | "mono" => Some(MONO),
        _ => None,
    }
}

/// The families a registry value name lists: `Arial Bold (TrueType)` is
/// `Arial Bold`, `Cambria & Cambria Math (TrueType)` both.
fn families_of(value_name: &str) -> impl Iterator<Item = &str> {
    let name = match value_name.rfind(" (") {
        Some(i) if value_name.ends_with(')') => &value_name[..i],
        _ => value_name,
    };
    name.split(" & ").map(str::trim)
}

/// A registry value's file: a path, or a name in the fonts folder.
fn file_of(value: &str, fonts_dir: &Path) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        fonts_dir.join(path)
    }
}

/// The file of `family` among the registry's `(value name, file)` entries.
fn listed(entries: &[(String, PathBuf)], family: &str) -> Option<PathBuf> {
    entries.iter().find(|(name, _)| families_of(name).any(|f| f.eq_ignore_ascii_case(family))).map(|(_, file)| file.clone())
}

/// The first of `known` the system has: listed, or in the fonts folder.
fn first(entries: &[(String, PathBuf)], fonts_dir: &Path, known: &[Known]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for (family, files) in known {
        let found =
            listed(entries, family).filter(|p| p.is_file()).or_else(|| files.iter().map(|f| fonts_dir.join(f)).find(|p| p.is_file()));
        if let Some(p) = found.filter(|p| !out.contains(p)) {
            out.push(p);
        }
    }
    out
}

/// The fonts to try for a character the chosen font lacks, best first.
fn fallbacks(entries: &[(String, PathBuf)], fonts_dir: &Path) -> Vec<PathBuf> {
    first(entries, fonts_dir, &[SANS, FALLBACK].concat())
}

#[cfg(windows)]
pub use system::{family, for_char};

#[cfg(windows)]
mod system {
    use super::*;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
    };

    /// The registry's fonts, the machine's and then the user's, read once.
    fn entries() -> &'static [(String, PathBuf)] {
        static ENTRIES: OnceLock<Vec<(String, PathBuf)>> = OnceLock::new();
        ENTRIES.get_or_init(|| {
            let dir = fonts_dir();
            [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
                .into_iter()
                .flat_map(read_key)
                .map(|(name, value)| (name, file_of(&value, &dir)))
                .collect()
        })
    }

    fn fonts_dir() -> PathBuf {
        let windir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).unwrap_or_else(|| r"C:\Windows".into());
        PathBuf::from(windir).join("Fonts")
    }

    /// The string values of `root\...\CurrentVersion\Fonts`.
    fn read_key(root: HKEY) -> Vec<(String, String)> {
        let subkey: Vec<u16> = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts".encode_utf16().chain([0]).collect();
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `subkey` is NUL-terminated; `key` is closed below.
        if unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &mut key) } != ERROR_SUCCESS {
            return Vec::new();
        }
        let mut out = Vec::new();
        let (mut name, mut data) = (vec![0u16; 16384], vec![0u16; 2048]);
        for index in 0.. {
            let (mut name_len, mut data_len, mut kind) = (name.len() as u32, (data.len() * 2) as u32, 0u32);
            // SAFETY: the buffers are as long as the lengths passed say.
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr().cast(),
                    &mut data_len,
                )
            };
            if status != ERROR_SUCCESS {
                // ERROR_MORE_DATA: a value too long to be a file name.
                if status == windows_sys::Win32::Foundation::ERROR_MORE_DATA {
                    continue;
                }
                break;
            }
            if kind == REG_SZ {
                let value = &data[..(data_len as usize / 2)];
                let value = value.split(|&u| u == 0).next().unwrap_or_default();
                out.push((String::from_utf16_lossy(&name[..name_len as usize]), String::from_utf16_lossy(value)));
            }
        }
        // SAFETY: opened above.
        unsafe { RegCloseKey(key) };
        out
    }

    pub fn family(name: &str) -> Option<PathBuf> {
        match generic(name) {
            Some(known) => first(entries(), &fonts_dir(), known).into_iter().next(),
            None => listed(entries(), name).filter(|p| p.is_file()),
        }
    }

    pub fn for_char(_: char) -> Vec<PathBuf> {
        static ALL: OnceLock<Vec<PathBuf>> = OnceLock::new();
        ALL.get_or_init(|| fallbacks(entries(), &fonts_dir())).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_names_list_their_families() {
        assert_eq!(families_of("Arial Bold (TrueType)").collect::<Vec<_>>(), ["Arial Bold"]);
        assert_eq!(families_of("Cambria & Cambria Math (TrueType)").collect::<Vec<_>>(), ["Cambria", "Cambria Math"]);
        assert_eq!(families_of("Noto Sans (OpenType)").collect::<Vec<_>>(), ["Noto Sans"]);
        assert_eq!(families_of("Plain").collect::<Vec<_>>(), ["Plain"]);
    }

    #[test]
    fn registry_files_are_paths_or_names_in_the_fonts_folder() {
        let dir = Path::new("/w/Fonts");
        assert_eq!(file_of("arial.ttf", dir), Path::new("/w/Fonts/arial.ttf"));
        assert_eq!(file_of("/usr/share/fonts/a.ttf", dir), Path::new("/usr/share/fonts/a.ttf"));
    }

    #[test]
    fn a_family_is_found_by_its_exact_name_in_any_case() {
        let entries = [("Arial Bold (TrueType)", "bd.ttf"), ("ARIAL (TrueType)", "a.ttf")].map(|(n, f)| (n.into(), PathBuf::from(f)));
        assert_eq!(listed(&entries, "arial"), Some(PathBuf::from("a.ttf")));
        assert_eq!(listed(&entries, "Arial Black"), None);
    }

    #[test]
    fn known_families_come_from_the_registry_or_the_fonts_folder_once_each() {
        let dir = std::env::temp_dir().join(format!("popamp-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["tahoma.ttf", "host.ttf", "msyh.ttf"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        // Segoe UI is missing; Arial is listed as the same file as Tahoma.
        let entries = [("Arial (TrueType)".to_string(), dir.join("host.ttf")), ("Tahoma (TrueType)".to_string(), dir.join("host.ttf"))];
        assert_eq!(first(&entries, &dir, SANS), [dir.join("host.ttf")]);
        assert_eq!(first(&[], &dir, SANS), [dir.join("tahoma.ttf")]);
        // Characters the sans-serif fonts lack come from the script ones.
        assert_eq!(fallbacks(&entries, &dir), [dir.join("host.ttf"), dir.join("msyh.ttf")]);
        assert_eq!(generic("Sans-Serif").map(|k| k[0].0), Some("Segoe UI"));
        assert!(generic("Arial").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
