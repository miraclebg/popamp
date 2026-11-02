//! Browsing and downloading skins from the Winamp Skin Museum
//! (skins.webamp.org) through its public GraphQL API. The skins belong to
//! their authors; the user picks one and Popamp fetches it for them.
//! Adapted from Ferramp's `museum.rs` (see THIRD_PARTY.md).

use crate::model::{AppEvent, Events};
use popamp_audio::stream::{fetch_bytes, post_json};
use popamp_skin::Bitmap;
use std::path::{Path, PathBuf};

const API: &str = "https://api.webamp.org/graphql";
const FILES: &str = "https://r2.webampskins.org";
/// Skins per request.
pub const PAGE: usize = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The museum's id: always 32 hex digits (checked on arrival).
    pub md5: String,
    pub filename: String,
}

impl Entry {
    /// The file name without its extension, for lists.
    pub fn name(&self) -> String {
        match self.filename.rsplit_once('.') {
            Some((stem, ext)) if ["wsz", "zip"].contains(&ext.to_lowercase().as_str()) => stem.to_string(),
            _ => self.filename.clone(),
        }
    }
}

/// Work for a background thread.
#[derive(Debug, Clone)]
pub enum Job {
    /// One page of the museum's own order (empty query) or of a search.
    Page {
        query: String,
        offset: usize,
    },
    Screenshot {
        md5: String,
    },
    /// Download into `dir` (the skins folder).
    Download {
        entry: Entry,
        dir: PathBuf,
    },
}

/// A finished job, delivered to the event loop.
#[derive(Debug)]
pub enum Fetched {
    Page { query: String, offset: usize, result: Result<Vec<Entry>, String> },
    Screenshot { md5: String, result: Result<Bitmap, String> },
    Download { name: String, result: Result<PathBuf, String> },
}

/// Runs `job` on its own thread and sends the result as [`AppEvent::Museum`].
pub fn start(job: Job, proxy: String, events: Events) {
    let spawned = std::thread::Builder::new().name("popamp-museum".into()).spawn(move || {
        events.send(AppEvent::Museum(run(job, &proxy)));
    });
    if let Err(e) = spawned {
        log::warn!("cannot start a museum download: {e}");
    }
}

fn run(job: Job, proxy: &str) -> Fetched {
    match job {
        Job::Page { query, offset } => {
            let result = post_json(API, proxy, &page_query(&query, offset)).and_then(|v| parse_page(&v));
            Fetched::Page { query, offset, result }
        }
        Job::Screenshot { md5 } => {
            let result = fetch_bytes(&format!("{FILES}/screenshots/{md5}.png"), proxy, 2 << 20).and_then(|b| Bitmap::decode(&b));
            Fetched::Screenshot { md5, result }
        }
        Job::Download { entry, dir } => {
            let result =
                fetch_bytes(&format!("{FILES}/skins/{}.wsz", entry.md5), proxy, 16 << 20).and_then(|data| save(&entry, &dir, &data));
            Fetched::Download { name: entry.name(), result }
        }
    }
}

/// The GraphQL request for one page. Adult skins are left out.
fn page_query(query: &str, offset: usize) -> serde_json::Value {
    if query.trim().is_empty() {
        serde_json::json!({
            "query": "query($first: Int!, $offset: Int!) { list: skins(sort: MUSEUM, \
                      first: $first, offset: $offset) { nodes { md5 filename nsfw } } }",
            "variables": { "first": PAGE, "offset": offset },
        })
    } else {
        serde_json::json!({
            "query": "query($q: String!, $first: Int!, $offset: Int!) { list: \
                      search_classic_skins(query: $q, first: $first, offset: $offset, \
                      nsfw: false) { md5 filename nsfw } }",
            "variables": { "q": query.trim(), "first": PAGE, "offset": offset },
        })
    }
}

/// Reads either reply shape (browse: `list.nodes`, search: `list`).
fn parse_page(v: &serde_json::Value) -> Result<Vec<Entry>, String> {
    if let Some(msg) = v["errors"][0]["message"].as_str() {
        return Err(msg.to_string());
    }
    let list = &v["data"]["list"];
    let nodes = list["nodes"].as_array().or(list.as_array()).ok_or("unexpected reply from the museum")?;
    Ok(nodes
        .iter()
        .filter(|n| !n["nsfw"].as_bool().unwrap_or(false))
        .filter_map(|n| {
            let (md5, filename) = (n["md5"].as_str()?, n["filename"].as_str()?);
            is_md5(md5).then(|| Entry { md5: md5.to_lowercase(), filename: filename.to_string() })
        })
        .collect())
}

/// Museum ids end up in URLs and file names, so only real MD5 hex passes.
fn is_md5(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A safe file name for the skins folder.
fn file_name(e: &Entry) -> String {
    let stem: String = e.name().chars().map(|c| if c.is_alphanumeric() || " ._-()[]".contains(c) { c } else { '_' }).collect();
    let stem = stem.trim().trim_start_matches('.');
    let stem = if stem.is_empty() { &e.md5[..8] } else { stem };
    format!("{stem}.wsz")
}

/// Saves a downloaded skin after checking that it is one. A different skin
/// with the same name is kept; the new one gets the id appended.
fn save(e: &Entry, dir: &Path, data: &[u8]) -> Result<PathBuf, String> {
    popamp_skin::check_archive(data).map_err(|err| format!("not a usable skin ({err})"))?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut path = dir.join(file_name(e));
    if path.exists() && std::fs::read(&path).ok().as_deref() != Some(data) {
        path = dir.join(format!("{}-{}.wsz", file_name(e).trim_end_matches(".wsz"), &e.md5[..8]));
    }
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(f: &str) -> Entry {
        Entry { md5: "0123456789abcdef0123456789abcdef".into(), filename: f.into() }
    }

    #[test]
    fn parses_both_reply_shapes_and_drops_unsafe_entries() {
        let browse = serde_json::json!({"data": {"list": {"nodes": [
            {"md5": "5e4f10275dcb1fb211d4a8b4f1bda236", "filename": "base-2.91.wsz", "nsfw": false},
            {"md5": "d6010aa35bed659bc1311820daa4b341", "filename": "x.wsz", "nsfw": true},
        ]}}});
        let r = parse_page(&browse).unwrap();
        assert_eq!(r.iter().map(Entry::name).collect::<Vec<_>>(), ["base-2.91"]);
        let search = serde_json::json!({"data": {"list": [
            {"md5": "../../etc/passwd", "filename": "evil.wsz"},
            {"md5": "D6010AA35BED659BC1311820DAA4B341", "filename": "Bento.zip"},
        ]}});
        let r = parse_page(&search).unwrap();
        assert_eq!((r[0].name(), r[0].md5.as_str(), r.len()), ("Bento".into(), "d6010aa35bed659bc1311820daa4b341", 1));
        assert_eq!(parse_page(&serde_json::json!({"errors": [{"message": "boom"}]})).unwrap_err(), "boom");
        assert!(parse_page(&serde_json::json!({"data": {}})).is_err());
    }

    #[test]
    fn searches_only_when_there_is_a_query() {
        assert!(page_query("", 0)["query"].as_str().unwrap().contains("sort: MUSEUM"));
        let q = page_query(" bento ", 60);
        assert_eq!((q["variables"]["q"].as_str(), q["variables"]["offset"].as_u64()), (Some("bento"), Some(60)));
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name(&entry("Bento Classified.wsz")), "Bento Classified.wsz");
        assert_eq!(file_name(&entry("../../x/y.zip")), "_.._x_y.wsz");
        assert_eq!(file_name(&entry("a\\b:c*.wsz")), "a_b_c_.wsz");
        assert_eq!(file_name(&entry("...")), "01234567.wsz");
    }

    /// A zip with a 1x1 main window bitmap: enough to count as a skin.
    fn tiny_skin() -> Vec<u8> {
        use std::io::Write;
        let mut png = Vec::new();
        image::RgbImage::new(1, 1).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        w.start_file("main.bmp", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
        w.write_all(&png).unwrap();
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn saves_only_real_skins_and_keeps_other_files() {
        let dir = std::env::temp_dir().join(format!("popamp-museum-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(save(&entry("a.wsz"), &dir, b"<html>").unwrap_err().contains("not a usable skin"));
        let skin = tiny_skin();
        let first = save(&entry("a.wsz"), &dir, &skin).unwrap();
        assert_eq!(first, dir.join("a.wsz"));
        assert_eq!(save(&entry("a.wsz"), &dir, &skin).unwrap(), first, "same skin again");
        std::fs::write(&first, b"someone else's").unwrap();
        assert_eq!(save(&entry("a.wsz"), &dir, &skin).unwrap(), dir.join("a-01234567.wsz"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Talks to the real museum: `cargo test -- --ignored live_museum`.
    #[test]
    #[ignore]
    fn live_museum() {
        let Fetched::Page { result, .. } = run(Job::Page { query: String::new(), offset: 0 }, "") else { panic!() };
        let first = result.unwrap();
        assert!(first.len() > 10);
        let Fetched::Page { result, .. } = run(Job::Page { query: "bento".into(), offset: 0 }, "") else { panic!() };
        assert!(result.unwrap().iter().any(|e| e.filename.to_lowercase().contains("bento")));
        let Fetched::Screenshot { result, .. } = run(Job::Screenshot { md5: first[0].md5.clone() }, "") else { panic!() };
        assert_eq!(result.map(|b| (b.w, b.h)), Ok((275, 348)));
    }
}
