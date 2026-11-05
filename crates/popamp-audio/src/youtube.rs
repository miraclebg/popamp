//! YouTube playback: `yt-dlp` finds the audio stream, `ffmpeg` decodes it to
//! 48 kHz stereo `f32` on a pipe.
//!
//! Stream URLs expire after a few hours, so playlists keep the YouTube page
//! URL and it is resolved again (with a short cache) whenever it is played.

use crate::pipe::{Pipe, START};
use parking_lot::{Mutex, RwLock};
use popamp_core::meta::TrackMeta;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;
const CACHE_TTL: Duration = Duration::from_secs(3600);
/// Upper bound for expanding playlists and channels.
const MAX_PLAYLIST: usize = 500;

#[derive(Debug, Clone)]
pub struct Config {
    pub ytdlp: String,
    pub ffmpeg: String,
    /// yt-dlp format selector.
    pub format: String,
    /// HTTP proxy (`[user:pass@]host:port`), empty for none.
    pub proxy: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { ytdlp: "yt-dlp".into(), ffmpeg: "ffmpeg".into(), format: "bestaudio[ext=m4a]/bestaudio".into(), proxy: String::new() }
    }
}

fn config() -> &'static RwLock<Config> {
    static C: OnceLock<RwLock<Config>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(Config::default()))
}

pub fn configure(c: Config) {
    *config().write() = c;
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?.split(':').next()?;
    Some(host.to_ascii_lowercase())
}

/// True for youtube.com / youtu.be / music.youtube.com / youtube-nocookie links.
pub fn is_youtube(url: &str) -> bool {
    let Some(host) = host_of(url) else { return false };
    let host = host.strip_prefix("www.").unwrap_or(&host);
    matches!(host, "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "youtube-nocookie.com")
}

/// True when the URL names one video (as opposed to a playlist or channel).
pub fn is_single_video(url: &str) -> bool {
    let Some(host) = host_of(url) else { return false };
    let path = url.split_once("://").map(|(_, r)| r).and_then(|r| r.find('/').map(|i| &r[i..])).unwrap_or("");
    if host.ends_with("youtu.be") {
        return path.len() > 1;
    }
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    path.starts_with("/shorts/")
        || path.starts_with("/live/")
        || path.starts_with("/embed/")
        || query.split('&').any(|kv| kv.starts_with("v="))
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub stream_url: String,
    pub headers: Vec<(String, String)>,
    pub meta: TrackMeta,
    pub is_live: bool,
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Resolved)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Resolved)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn run_ytdlp(args: &[&str]) -> Result<serde_json::Value, String> {
    let cfg = config().read().clone();
    let mut cmd = Command::new(&cfg.ytdlp);
    cmd.args(["--no-warnings", "--no-progress"]);
    if !cfg.proxy.trim().is_empty() {
        cmd.args(["--proxy", &format!("http://{}", cfg.proxy.trim())]);
    }
    cmd.args(args).stdin(Stdio::null());
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{} not found: install yt-dlp to play YouTube links", cfg.ytdlp)
        } else {
            format!("cannot run {}: {e}", cfg.ytdlp)
        }
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().rev().find(|l| l.contains("ERROR")).or_else(|| err.lines().last()).unwrap_or("yt-dlp failed");
        return Err(line.trim().trim_start_matches("ERROR: ").to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("yt-dlp returned bad JSON: {e}"))
}

fn str_field(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string))
}

fn meta_from_json(v: &serde_json::Value) -> TrackMeta {
    TrackMeta {
        title: str_field(v, &["track", "title"]),
        artist: str_field(v, &["artist", "creator", "uploader", "channel"]),
        album: str_field(v, &["album"]),
        year: str_field(v, &["release_year"]).or_else(|| v.get("release_year").and_then(|y| y.as_u64()).map(|y| y.to_string())),
        genre: str_field(v, &["genre"]),
        comment: str_field(v, &["webpage_url"]),
        length_ms: v.get("duration").and_then(|d| d.as_f64()).filter(|d| *d > 0.0).map(|d| (d * 1000.0) as u64),
        bitrate_kbps: v.get("abr").and_then(|b| b.as_f64()).map(|b| b.round() as u32),
        sample_rate: v.get("asr").and_then(|r| r.as_u64()).map(|r| r as u32),
        channels: v.get("audio_channels").and_then(|c| c.as_u64()).map(|c| c as u16),
        codec: str_field(v, &["acodec"]).map(|c| c.split('.').next().unwrap_or(&c).to_uppercase()),
        ..Default::default()
    }
}

/// Resolves a video URL to its audio stream (cached for an hour).
pub fn resolve(url: &str) -> Result<Resolved, String> {
    if let Some((at, r)) = cache().lock().get(url) {
        if at.elapsed() < CACHE_TTL {
            return Ok(r.clone());
        }
    }
    let format = config().read().format.clone();
    let v = run_ytdlp(&["-j", "--no-playlist", "-f", &format, url])?;
    let stream_url = str_field(&v, &["url"]).ok_or("yt-dlp found no audio stream")?;
    let headers = v
        .get("http_headers")
        .and_then(|h| h.as_object())
        .map(|h| h.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect())
        .unwrap_or_default();
    let is_live = v.get("is_live").and_then(|l| l.as_bool()).unwrap_or(false);
    let r = Resolved { stream_url, headers, meta: meta_from_json(&v), is_live };
    let mut c = cache().lock();
    c.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
    c.insert(url.to_string(), (Instant::now(), r.clone()));
    Ok(r)
}

/// Tags for the playlist (resolves the video).
pub fn read_meta(url: &str) -> Result<TrackMeta, String> {
    resolve(url).map(|r| r.meta)
}

/// One item of a playlist/channel listing.
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub url: String,
    pub title: Option<String>,
    pub length_ms: Option<u64>,
}

/// Lists the videos of a playlist or channel URL without resolving them.
pub fn list(url: &str) -> Result<Vec<ListItem>, String> {
    let end = MAX_PLAYLIST.to_string();
    let v = run_ytdlp(&["-J", "--flat-playlist", "--playlist-end", &end, url])?;
    Ok(parse_listing(&v))
}

fn parse_listing(v: &serde_json::Value) -> Vec<ListItem> {
    let Some(entries) = v.get("entries").and_then(|e| e.as_array()) else {
        // Not a playlist after all: the page itself is the video.
        return str_field(v, &["webpage_url", "original_url"])
            .map(|url| vec![ListItem { url, title: str_field(v, &["title"]), length_ms: meta_from_json(v).length_ms }])
            .unwrap_or_default();
    };
    entries
        .iter()
        .filter_map(|e| {
            let url = match str_field(e, &["url"]) {
                Some(u) if u.starts_with("http") => u,
                _ => format!("https://www.youtube.com/watch?v={}", str_field(e, &["id"])?),
            };
            let title = match (str_field(e, &["uploader", "channel"]), str_field(e, &["title"])) {
                (Some(a), Some(t)) => Some(format!("{a} - {t}")),
                (_, t) => t,
            };
            Some(ListItem { url, title, length_ms: e.get("duration").and_then(|d| d.as_f64()).map(|d| (d * 1000.0) as u64) })
        })
        .collect()
}

/// How to play a video: `ffmpeg` decodes its audio stream to stereo `f32`.
pub fn pipe(url: &str) -> Result<Pipe, String> {
    Ok(ffmpeg_pipe(resolve(url)?, &config().read()))
}

fn ffmpeg_pipe(resolved: Resolved, cfg: &Config) -> Pipe {
    let mut args: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"].map(String::from).into();
    args.extend(["-reconnect", "1", "-reconnect_streamed", "1", "-reconnect_delay_max", "5"].map(String::from));
    let mut extra = String::new();
    for (k, v) in &resolved.headers {
        if k.eq_ignore_ascii_case("user-agent") {
            args.extend(["-user_agent".into(), v.clone()]);
        } else {
            extra.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    if !extra.is_empty() {
        args.extend(["-headers".into(), extra]);
    }
    if !cfg.proxy.trim().is_empty() {
        args.extend(["-http_proxy".into(), format!("http://{}", cfg.proxy.trim())]);
    }
    if !resolved.is_live {
        args.extend(["-ss".into(), START.into()]);
    }
    args.extend(["-i".into(), resolved.stream_url]);
    args.extend(["-vn", "-f", "f32le", "-ac", "2", "-ar"].map(String::from));
    args.extend([RATE.to_string(), "pipe:1".into()]);
    let mut meta = resolved.meta;
    meta.codec.get_or_insert_with(|| "YOUTUBE".into());
    if resolved.is_live {
        meta.length_ms = None;
    }
    Pipe { program: cfg.ffmpeg.clone(), args, rate: RATE, seekable: !resolved.is_live, meta }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_youtube_urls() {
        for u in [
            "https://www.youtube.com/watch?v=abc",
            "http://youtube.com/playlist?list=PL1",
            "https://music.youtube.com/watch?v=x&list=y",
            "https://youtu.be/abc?t=3",
            "https://m.youtube.com/@channel",
            "https://user@www.youtube.com:443/watch?v=a",
        ] {
            assert!(is_youtube(u), "{u}");
        }
        for u in
            ["https://notyoutube.com/watch?v=a", "https://example.com/youtube.com", "/music/youtube.com.mp3", "http://radio:8000/stream"]
        {
            assert!(!is_youtube(u), "{u}");
        }
    }

    #[test]
    fn single_vs_list() {
        assert!(is_single_video("https://www.youtube.com/watch?v=abc&list=PL1"));
        assert!(is_single_video("https://youtu.be/abc"));
        assert!(is_single_video("https://www.youtube.com/shorts/abc"));
        assert!(!is_single_video("https://www.youtube.com/playlist?list=PL1"));
        assert!(!is_single_video("https://www.youtube.com/@someone/videos"));
    }

    #[test]
    fn parses_listings_and_meta() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"entries":[
                {"id":"aaa","title":"One","uploader":"Band","duration":61.5},
                {"url":"https://www.youtube.com/watch?v=bbb","title":"Two"},
                {"title":"no id"}
            ]}"#,
        )
        .unwrap();
        let items = parse_listing(&v);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url, "https://www.youtube.com/watch?v=aaa");
        assert_eq!(items[0].title.as_deref(), Some("Band - One"));
        assert_eq!(items[0].length_ms, Some(61_500));
        assert_eq!(items[1].title.as_deref(), Some("Two"));

        let m = meta_from_json(&serde_json::json!({"title":"Vid","uploader":"Chan","duration":19,"acodec":"mp4a.40.2","abr":129.8}));
        assert_eq!(m.title.as_deref(), Some("Vid"));
        assert_eq!(m.artist.as_deref(), Some("Chan"));
        assert_eq!(m.length_ms, Some(19_000));
        assert_eq!(m.codec.as_deref(), Some("MP4A"));
        assert_eq!(m.bitrate_kbps, Some(130));
    }

    #[test]
    fn plays_a_video_through_ffmpeg_from_the_start_asked() {
        let resolved = Resolved {
            stream_url: "https://media/a".into(),
            headers: vec![("User-Agent".into(), "UA".into()), ("Referer".into(), "r".into())],
            meta: TrackMeta { length_ms: Some(5000), ..Default::default() },
            is_live: false,
        };
        let cfg = Config { proxy: "h:1".into(), ..Config::default() };
        let p = ffmpeg_pipe(resolved.clone(), &cfg);
        let at = |a: &str| p.args.iter().position(|x| x == a).unwrap();
        assert_eq!(p.program, "ffmpeg");
        assert_eq!(p.args[at("-ss") + 1], START);
        assert!(at("-ss") < at("-i"));
        assert_eq!(p.args[at("-i") + 1], "https://media/a");
        assert_eq!(p.args[at("-user_agent") + 1], "UA");
        assert_eq!(p.args[at("-headers") + 1], "Referer: r\r\n");
        assert_eq!(p.args[at("-http_proxy") + 1], "http://h:1");
        assert_eq!((p.seekable, p.meta.length_ms, p.meta.codec.as_deref()), (true, Some(5000), Some("YOUTUBE")));

        let live = ffmpeg_pipe(Resolved { is_live: true, ..resolved }, &Config::default());
        assert!(!live.args.iter().any(|a| a == "-ss" || a == "-http_proxy"));
        assert_eq!((live.seekable, live.meta.length_ms), (false, None));
    }
}
