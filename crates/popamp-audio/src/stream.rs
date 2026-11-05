//! HTTP / SHOUTcast / Icecast streams with inline ICY metadata.

use parking_lot::Mutex;
use std::io::{self, Read};
use std::sync::Arc;
use symphonia::core::io::{MediaSource, ReadOnlySource};

#[derive(Debug, Clone, Default)]
pub struct StreamHeaders {
    pub name: Option<String>,
    pub genre: Option<String>,
    pub url: Option<String>,
    pub bitrate: Option<u32>,
    pub content_type: String,
}

/// Latest `StreamTitle` seen in the stream; the engine polls it.
pub type TitleSlot = Arc<Mutex<Option<String>>>;

/// Strips ICY metadata blocks interleaved every `metaint` bytes.
struct IcyReader<R> {
    inner: R,
    metaint: usize,
    until_meta: usize,
    title: TitleSlot,
}

impl<R: Read> Read for IcyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.metaint == 0 {
            return self.inner.read(buf);
        }
        if self.until_meta == 0 {
            let mut len = [0u8; 1];
            if self.inner.read(&mut len)? == 0 {
                return Ok(0);
            }
            let meta_len = len[0] as usize * 16;
            if meta_len > 0 {
                let mut meta = vec![0u8; meta_len];
                self.inner.read_exact(&mut meta)?;
                if let Some(t) = parse_stream_title(&meta) {
                    *self.title.lock() = Some(t);
                }
            }
            self.until_meta = self.metaint;
        }
        let want = buf.len().min(self.until_meta);
        let n = self.inner.read(&mut buf[..want])?;
        self.until_meta -= n;
        Ok(n)
    }
}

pub fn parse_stream_title(meta: &[u8]) -> Option<String> {
    let text = popamp_core::text::decode_lossy(meta);
    let text = text.trim_end_matches('\0');
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    // The value ends at "';" (titles may contain single quotes).
    let end = rest.find("';").or_else(|| rest.rfind('\''))?;
    let title = rest[..end].trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// An opened stream: media source, format hint, headers and the title slot.
pub type Opened = (Box<dyn MediaSource>, Option<&'static str>, StreamHeaders, TitleSlot);

/// Connects to `url`. Returns a media source, an extension hint and headers.
pub fn open(url: &str, proxy: &str) -> Result<Opened, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(20))
        .user_agent(concat!("Popamp/", env!("CARGO_PKG_VERSION")));
    if !proxy.trim().is_empty() {
        let p = ureq::Proxy::new(proxy.trim()).map_err(|e| format!("bad proxy: {e}"))?;
        builder = builder.proxy(p);
    }
    let resp = builder.build().get(url).set("Icy-MetaData", "1").call().map_err(|e| format!("cannot connect: {e}"))?;
    let header = |k: &str| resp.header(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let headers = StreamHeaders {
        name: header("icy-name"),
        genre: header("icy-genre"),
        url: header("icy-url"),
        bitrate: header("icy-br").and_then(|b| b.split(',').next().and_then(|v| v.trim().parse().ok())),
        content_type: resp.content_type().to_ascii_lowercase(),
    };
    // A web page (a video site without its plug-in) is a link for a
    // plug-in, not a stream.
    if headers.content_type == "text/html" {
        return Err("no plug-in plays this link".into());
    }
    let metaint: usize = header("icy-metaint").and_then(|v| v.parse().ok()).unwrap_or(0);
    let ext = match headers.content_type.as_str() {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/aac" | "audio/aacp" | "audio/x-aac" => Some("aac"),
        "audio/ogg" | "application/ogg" | "audio/vorbis" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        _ => url.rsplit('.').next().and_then(|e| match e.to_ascii_lowercase().as_str() {
            "mp3" => Some("mp3"),
            "ogg" | "oga" => Some("ogg"),
            "aac" => Some("aac"),
            "flac" => Some("flac"),
            _ => None,
        }),
    };
    let title: TitleSlot = Arc::new(Mutex::new(None));
    let reader = IcyReader { inner: resp.into_reader(), metaint, until_meta: metaint, title: title.clone() };
    Ok((Box::new(ReadOnlySource::new(reader)), ext, headers, title))
}

/// An HTTP client for small downloads, through `proxy` when set.
fn agent(proxy: &str, timeout_s: u64) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(timeout_s))
        .user_agent(concat!("Popamp/", env!("CARGO_PKG_VERSION")));
    if !proxy.trim().is_empty() {
        builder = builder.proxy(ureq::Proxy::new(proxy.trim()).map_err(|e| e.to_string())?);
    }
    Ok(builder.build())
}

fn read_body(resp: ureq::Response, max: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    resp.into_reader().take(max).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Downloads a small text resource (remote playlists).
pub fn fetch_text(url: &str, proxy: &str) -> Result<String, String> {
    Ok(popamp_core::text::decode_lossy(&fetch_bytes(url, proxy, 1 << 20)?))
}

/// Downloads at most `max` bytes from `url`.
pub fn fetch_bytes(url: &str, proxy: &str, max: u64) -> Result<Vec<u8>, String> {
    let resp = agent(proxy, 60)?.get(url).call().map_err(|e| e.to_string())?;
    read_body(resp, max)
}

/// POSTs a JSON body (a GraphQL query, say) and parses the JSON reply.
pub fn post_json(url: &str, proxy: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let resp =
        agent(proxy, 30)?.post(url).set("Content-Type", "application/json").send_string(&body.to_string()).map_err(|e| e.to_string())?;
    serde_json::from_slice(&read_body(resp, 4 << 20)?).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icy_title_parsing() {
        assert_eq!(parse_stream_title(b"StreamTitle='Artist - It's Song';StreamUrl='';\0\0").as_deref(), Some("Artist - It's Song"));
        assert_eq!(parse_stream_title(b"StreamTitle='';"), None);
        assert_eq!(parse_stream_title(b"garbage"), None);
    }

    #[test]
    fn strips_metadata_blocks() {
        let mut data = b"abcd".to_vec();
        let meta = b"StreamTitle='X';";
        data.push(1); // 16 bytes of metadata follow
        data.extend_from_slice(meta);
        data.extend_from_slice(b"efgh");
        data.push(0);
        data.extend_from_slice(b"ij");
        let title: TitleSlot = Default::default();
        let mut r = IcyReader { inner: &data[..], metaint: 4, until_meta: 4, title: title.clone() };
        let mut out = Vec::new();
        r.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"abcdefghij");
        assert_eq!(title.lock().as_deref(), Some("X"));
    }
}
