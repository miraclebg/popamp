//! System fonts through fontconfig's `fc-match`.

use std::path::PathBuf;

/// The file of the font family `name`, or of what fontconfig puts in its
/// place (generic names such as `sans-serif` included).
pub fn family(name: &str) -> Option<PathBuf> {
    fc_match(name)
}

/// The fonts to try for `c`, best first.
pub fn for_char(c: char) -> Vec<PathBuf> {
    fc_match(&format!(":charset={:x}", c as u32)).into_iter().collect()
}

/// A pattern such as `DejaVu Sans` or `:charset=4e2d`, matched.
fn fc_match(pattern: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    let path = String::from_utf8(out.stdout).ok()?;
    let path = PathBuf::from(path.trim());
    path.is_file().then_some(path)
}
