//! Playlist files: M3U / M3U8, PLS and XSPF.

use crate::playlist::{is_url, Entry};
use crate::text::{decode_lossy, path_to_file_url, percent_decode, xml_escape, xml_unescape};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistFormat {
    M3u,
    M3u8,
    Pls,
    Xspf,
}

impl PlaylistFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "m3u" => Self::M3u,
            "m3u8" => Self::M3u8,
            "pls" => Self::Pls,
            "xspf" => Self::Xspf,
            _ => return None,
        })
    }
}

pub fn is_playlist(path: &str) -> bool {
    PlaylistFormat::from_path(Path::new(path.split(['?', '#']).next().unwrap_or(path))).is_some()
}

/// One item read from a playlist file.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub location: String,
    pub title: Option<String>,
    pub length_ms: Option<u64>,
}

impl Item {
    pub fn into_entry(self) -> Entry {
        let mut e = Entry::new(self.location);
        e.file_title = self.title.filter(|t| !t.is_empty());
        e.length_ms = self.length_ms;
        if let Some(t) = &e.file_title {
            e.display = t.clone();
        }
        e
    }
}

/// Resolves a playlist reference relative to the playlist's directory.
fn resolve(base: Option<&Path>, raw: &str) -> String {
    let raw = raw.trim();
    if is_url(raw) {
        return raw.to_string();
    }
    if let Some(rest) = raw.strip_prefix("file://") {
        // file:///abs/path or file://localhost/abs/path
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        return percent_decode(rest);
    }
    let path = raw.replace('\\', "/");
    let p = Path::new(&path);
    if p.is_absolute() {
        return path;
    }
    match base {
        Some(dir) => normalize(&dir.join(p)).to_string_lossy().into_owned(),
        None => path,
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

pub fn parse(text: &str, format: PlaylistFormat, base: Option<&Path>) -> Vec<Item> {
    match format {
        PlaylistFormat::M3u | PlaylistFormat::M3u8 => parse_m3u(text, base),
        PlaylistFormat::Pls => parse_pls(text, base),
        PlaylistFormat::Xspf => parse_xspf(text, base),
    }
}

/// Guesses the format from content (for URLs without a useful extension).
pub fn sniff(text: &str) -> Option<PlaylistFormat> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with("#EXTM3U") {
        Some(PlaylistFormat::M3u8)
    } else if t.to_ascii_lowercase().starts_with("[playlist]") {
        Some(PlaylistFormat::Pls)
    } else if t.starts_with("<?xml") && t.contains("<playlist") {
        Some(PlaylistFormat::Xspf)
    } else {
        None
    }
}

pub fn read_file(path: &Path) -> io::Result<Vec<Item>> {
    let bytes = std::fs::read(path)?;
    let text = decode_lossy(&bytes);
    let format = PlaylistFormat::from_path(path).or_else(|| sniff(&text)).unwrap_or(PlaylistFormat::M3u);
    Ok(parse(&text, format, path.parent()))
}

fn parse_m3u(text: &str, base: Option<&Path>) -> Vec<Item> {
    let mut items = Vec::new();
    let mut pending: (Option<String>, Option<u64>) = (None, None);
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            let (len, title) = info.split_once(',').unwrap_or((info, ""));
            // The duration may be followed by attributes: `#EXTINF:-1 tvg-id="x",Title`.
            let secs: i64 = len.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()).map(|f| f as i64).unwrap_or(-1);
            pending = (Some(title.trim().to_string()), (secs >= 0).then(|| secs as u64 * 1000));
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let (title, length_ms) = std::mem::take(&mut pending);
        items.push(Item { location: resolve(base, line), title, length_ms });
    }
    items
}

fn parse_pls(text: &str, base: Option<&Path>) -> Vec<Item> {
    let ini = crate::ini::Ini::parse(text);
    let mut items = Vec::new();
    let declared: usize = ini.get("playlist", "NumberOfEntries").and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    // Scan beyond the declared count, many files get it wrong.
    let limit = declared.max(1) + 1000;
    let mut misses = 0;
    for i in 1..=limit {
        match ini.get("playlist", &format!("File{i}")) {
            Some(file) => {
                misses = 0;
                let title = ini.get("playlist", &format!("Title{i}")).map(str::to_string);
                let length_ms = ini
                    .get("playlist", &format!("Length{i}"))
                    .and_then(|s| s.trim().parse::<i64>().ok())
                    .filter(|&l| l >= 0)
                    .map(|l| l as u64 * 1000);
                items.push(Item { location: resolve(base, file), title, length_ms });
            }
            None => {
                misses += 1;
                if i > declared && misses > 50 {
                    break;
                }
            }
        }
    }
    items
}

fn tag_content<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after = &block[start + open.len()..];
    let gt = after.find('>')?;
    if after[..gt].ends_with('/') {
        return Some("");
    }
    let body = &after[gt + 1..];
    let end = body.find(&format!("</{tag}>"))?;
    Some(body[..end].trim())
}

fn parse_xspf(text: &str, base: Option<&Path>) -> Vec<Item> {
    let mut items = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<track") {
        let block_start = &rest[start..];
        let Some(end) = block_start.find("</track>") else { break };
        let block = &block_start[..end];
        if let Some(loc) = tag_content(block, "location") {
            let loc = xml_unescape(loc);
            let title = tag_content(block, "title").map(xml_unescape);
            let creator = tag_content(block, "creator").map(xml_unescape);
            let title = match (creator, title) {
                (Some(c), Some(t)) if !c.is_empty() => Some(format!("{c} - {t}")),
                (_, t) => t,
            };
            let length_ms = tag_content(block, "duration").and_then(|d| d.parse().ok());
            items.push(Item { location: resolve(base, &loc), title, length_ms });
        }
        rest = &block_start[end + "</track>".len()..];
    }
    items
}

fn relative_or_absolute(location: &str, base: Option<&Path>) -> String {
    if is_url(location) {
        return location.to_string();
    }
    if let Some(base) = base {
        if let Ok(rel) = Path::new(location).strip_prefix(base) {
            return rel.to_string_lossy().into_owned();
        }
    }
    location.to_string()
}

pub fn write(items: &[Item], format: PlaylistFormat, base: Option<&Path>) -> String {
    let mut out = String::new();
    match format {
        PlaylistFormat::M3u | PlaylistFormat::M3u8 => {
            out.push_str("#EXTM3U\n");
            for it in items {
                if it.title.is_some() || it.length_ms.is_some() {
                    let secs = it.length_ms.map(|l| (l / 1000) as i64).unwrap_or(-1);
                    out.push_str(&format!("#EXTINF:{secs},{}\n", it.title.as_deref().unwrap_or("")));
                }
                out.push_str(&relative_or_absolute(&it.location, base));
                out.push('\n');
            }
        }
        PlaylistFormat::Pls => {
            out.push_str("[playlist]\n");
            for (i, it) in items.iter().enumerate() {
                let n = i + 1;
                out.push_str(&format!("File{n}={}\n", relative_or_absolute(&it.location, base)));
                if let Some(t) = &it.title {
                    out.push_str(&format!("Title{n}={t}\n"));
                }
                let secs = it.length_ms.map(|l| (l / 1000) as i64).unwrap_or(-1);
                out.push_str(&format!("Length{n}={secs}\n"));
            }
            out.push_str(&format!("NumberOfEntries={}\nVersion=2\n", items.len()));
        }
        PlaylistFormat::Xspf => {
            out.push_str(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n  <trackList>\n",
            );
            for it in items {
                let loc = if is_url(&it.location) { it.location.clone() } else { path_to_file_url(&it.location) };
                out.push_str("    <track>\n");
                out.push_str(&format!("      <location>{}</location>\n", xml_escape(&loc)));
                if let Some(t) = &it.title {
                    out.push_str(&format!("      <title>{}</title>\n", xml_escape(t)));
                }
                if let Some(l) = it.length_ms {
                    out.push_str(&format!("      <duration>{l}</duration>\n"));
                }
                out.push_str("    </track>\n");
            }
            out.push_str("  </trackList>\n</playlist>\n");
        }
    }
    out
}

/// Writes a playlist file; M3U is written as UTF-8 too (lossless).
pub fn write_file(path: &Path, items: &[Item]) -> io::Result<()> {
    let format = PlaylistFormat::from_path(path).unwrap_or(PlaylistFormat::M3u8);
    let text = write(items, format, path.parent());
    crate::config::write_atomic(path, text.as_bytes())
}

/// A playlist as a web page: numbered titles with their lengths, and the
/// total, in the player's colours ("Generate HTML playlist").
pub fn html(items: &[Item]) -> String {
    let len = |i: &Item| i.length_ms.map(crate::text::format_duration_ms).unwrap_or_else(|| "?".into());
    let total: u64 = items.iter().filter_map(|i| i.length_ms).sum();
    let mut s = String::from(concat!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Popamp playlist</title>\n",
        "<style>body{background:#000040;color:#ffbf00;font-family:sans-serif}span{color:#fff}</style>\n",
        "</head><body>\n<h1>Popamp playlist</h1>\n",
    ));
    s += &format!("<p>{} tracks, total length {}</p>\n<ol>\n", items.len(), crate::text::format_duration_ms(total));
    for i in items {
        let title = i.title.as_deref().filter(|t| !t.is_empty()).unwrap_or(&i.location);
        s += &format!("<li>{} <span>({})</span></li>\n", xml_escape(title), len(i));
    }
    s + "</ol>\n</body></html>\n"
}

pub fn items_from_entries(entries: &[Entry]) -> Vec<Item> {
    entries
        .iter()
        .map(|e| Item { location: e.location.clone(), title: Some(e.display.clone()).filter(|t| !t.is_empty()), length_ms: e.length_ms })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_lists_tracks_with_lengths() {
        let items = [
            Item { location: "/a.mp3".into(), title: Some("Tom & Jerry <live>".into()), length_ms: Some(65_000) },
            Item { location: "/b.mp3".into(), title: None, length_ms: None },
        ];
        let html = html(&items);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("2 tracks, total length 1:05"));
        assert!(html.contains("<li>Tom &amp; Jerry &lt;live&gt; <span>(1:05)</span></li>"));
        assert!(html.contains("<li>/b.mp3 <span>(?)</span></li>"));
        assert!(html.trim_end().ends_with("</html>"));
    }

    fn sample() -> Vec<Item> {
        vec![
            Item { location: "/music/a b/01 – ä.mp3".into(), title: Some("Artist - Ä & <B>".into()), length_ms: Some(125_000) },
            Item { location: "http://radio.example/stream".into(), title: None, length_ms: None },
        ]
    }

    #[test]
    fn round_trip_all_formats() {
        for fmt in [PlaylistFormat::M3u8, PlaylistFormat::Pls, PlaylistFormat::Xspf] {
            let text = write(&sample(), fmt, None);
            let back = parse(&text, fmt, None);
            assert_eq!(back.len(), 2, "{fmt:?}");
            assert_eq!(back[0].location, sample()[0].location, "{fmt:?}");
            assert_eq!(back[0].title, sample()[0].title, "{fmt:?}");
            assert_eq!(back[0].length_ms, Some(125_000), "{fmt:?}");
            assert_eq!(back[1].location, "http://radio.example/stream", "{fmt:?}");
            assert_eq!(sniff(&text), Some(fmt).map(|f| if f == PlaylistFormat::M3u { PlaylistFormat::M3u8 } else { f }));
        }
    }

    #[test]
    fn relative_paths_resolve() {
        let base = Path::new("/home/u/lists");
        let items = parse("#EXTM3U\n../music/x.mp3\nsub\\y.flac\nfile:///abs/z%20z.ogg\n", PlaylistFormat::M3u, Some(base));
        let locs: Vec<_> = items.iter().map(|i| i.location.as_str()).collect();
        assert_eq!(locs, ["/home/u/music/x.mp3", "/home/u/lists/sub/y.flac", "/abs/z z.ogg"]);
        let written = write(&items, PlaylistFormat::M3u8, Some(base));
        assert!(written.contains("\nsub/y.flac\n"));
    }

    #[test]
    fn pls_tolerates_bad_counts() {
        let items = parse("[Playlist]\nNumberOfEntries=1\nFile1=http://a\nFile2=http://b\nTitle2=B\n", PlaylistFormat::Pls, None);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].title.as_deref(), Some("B"));
    }

    #[test]
    fn extinf_with_attributes() {
        let items = parse("#EXTM3U\n#EXTINF:-1 tvg-name=\"x\",Radio One\nhttp://r1\n", PlaylistFormat::M3u, None);
        assert_eq!(items[0].title.as_deref(), Some("Radio One"));
        assert_eq!(items[0].length_ms, None);
    }
}
