//! A plug-in for the host's tests: a source with `fixtures/test`, where
//! the path of a location under `https://test.invalid/` says what to do,
//! a dsp with `fixtures/test-dsp`, where its `mode` setting does, a
//! visual with `fixtures/test-visual` and a decoder with
//! `fixtures/test-decoder`.

use extism_pdk::*;
use popamp_plugin_api::{
    bytes_of, config as keys, samples_of, ExecOutput, ExecRequest, FileData, FileInfo, Format, HttpResponse, Listing, Play, Range,
    ReadRequest, Stream, Tags, Track, VERSION,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[host_fn]
extern "ExtismHost" {
    fn popamp_exec(req: Json<ExecRequest>) -> Json<ExecOutput>;
    fn popamp_now() -> Json<u64>;
    fn popamp_http(req: Json<popamp_plugin_api::HttpRequest>) -> Json<HttpResponse>;
    fn popamp_list(path: Json<String>) -> Json<Listing>;
    fn popamp_read(req: Json<ReadRequest>) -> Json<FileData>;
    fn popamp_input(req: Json<Range>) -> Json<FileData>;
}

fn path(location: &str) -> &str {
    location.strip_prefix("https://test.invalid").unwrap_or(location)
}

fn exec(command: &str, vars: &[(&str, &str)]) -> Result<ExecOutput, Error> {
    let vars: BTreeMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    Ok(unsafe { popamp_exec(Json(ExecRequest { command: command.into(), vars }))? }.0)
}

/// The error `popamp_http` answers for `url`, as this plug-in's own.
fn http_error(url: &str) -> Result<Play, Error> {
    let req = popamp_plugin_api::HttpRequest { method: "GET".into(), url: url.into(), headers: BTreeMap::new(), body: None };
    let out = unsafe { popamp_http(Json(req))? }.0;
    Err(Error::msg(out.error.unwrap_or_else(|| "no error".into())))
}

/// What the first file of the first folder the user allowed says.
fn first_file() -> Result<String, Error> {
    let roots = unsafe { popamp_list(Json(String::new()))? }.0;
    let root = roots.entries.first().ok_or_else(|| Error::msg("no folders"))?;
    let files = unsafe { popamp_list(Json(root.path.clone()))? }.0;
    let file = files.entries.first().ok_or_else(|| Error::msg("no files"))?;
    let data = unsafe { popamp_read(Json(ReadRequest { path: file.path.clone(), offset: 0, length: 100 }))? }.0;
    Ok(String::from_utf8_lossy(&data.data).into_owned())
}

#[plugin_fn]
pub fn api_version(_: String) -> FnResult<Json<popamp_plugin_api::Version>> {
    Ok(Json(VERSION))
}

#[plugin_fn]
pub fn expand(Json(location): Json<String>) -> FnResult<Json<Option<Vec<Track>>>> {
    Ok(Json((path(&location) == "/list").then(|| {
        (1..=2)
            .map(|i| Track { location: format!("{location}/{i}"), title: Some(format!("Track {i}")), length_ms: Some(i * 1000) })
            .collect()
    })))
}

#[plugin_fn]
pub fn describe(Json(location): Json<String>) -> FnResult<Json<Option<Tags>>> {
    Ok(Json((path(&location) != "/untagged").then(|| Tags { title: Some(format!("Title of {}", path(&location))), ..Default::default() })))
}

#[plugin_fn]
pub fn open(Json(location): Json<String>) -> FnResult<Json<Play>> {
    let play = match path(&location) {
        // The settings and the proxy reach the plug-in as its config.
        "/config" => {
            let greeting = config::get("greeting")?.unwrap_or_default();
            let proxy = config::get(keys::PROXY)?.unwrap_or_default();
            Play::Http { url: format!("http://{greeting}/{proxy}") }
        }
        "/now" => Play::Http { url: format!("http://now/{}", unsafe { popamp_now()? }.0) },
        // Asks a program for the rate, then plays 4 frames of silence.
        "/pipe" => {
            let out = exec("rate", &[("rate", "44100")])?;
            if let Some(e) = out.error {
                return Err(WithReturnCode::new(Error::msg(e), 1));
            }
            Play::Pipe {
                command: "silence".into(),
                vars: BTreeMap::new(),
                rate: out.stdout.trim().parse()?,
                seekable: false,
                tags: Tags { title: Some("Piped".into()), ..Default::default() },
            }
        }
        // A program that does not stop is stopped when the call's time is up.
        "/slow" => {
            let out = exec("slow", &[])?;
            return Err(WithReturnCode::new(Error::msg(out.error.unwrap_or_default()), 1));
        }
        "/forbidden" => {
            exec("rm", &[])?;
            unreachable!("the host refuses rm")
        }
        "/option" => {
            exec("rate", &[("rate", "--help")])?;
            unreachable!("the host refuses an option as a value")
        }
        "/extra" => {
            exec("rate", &[("rate", "1"), ("more", "x")])?;
            unreachable!("the host refuses a value the command has no place for")
        }
        "/files" => match first_file() {
            Ok(text) => Play::Http { url: format!("http://files/{text}") },
            Err(e) => return Err(WithReturnCode::new(e, 1)),
        },
        "/net-refused" => return Err(WithReturnCode::new(http_error("https://not-listed.invalid/").unwrap_err(), 1)),
        "/net-listed" => return Err(WithReturnCode::new(http_error("https://test.invalid/").unwrap_err(), 1)),
        // The runtime's own HTTP, with a host the manifest lists.
        "/extism-http" => {
            let req = extism_pdk::HttpRequest::new("https://test.invalid/");
            let e = http::request::<()>(&req, None).err().unwrap_or_else(|| Error::msg("it was let through"));
            return Err(WithReturnCode::new(e, 1));
        }
        // A source has no file to read.
        "/input" => return Err(WithReturnCode::new(input(0, 4).unwrap_err(), 1)),
        "/trap" => panic!("a bug in the plug-in"),
        "/loop" => loop {
            std::hint::black_box(());
        },
        other => return Err(WithReturnCode::new(Error::msg(format!("no track at {other}")), 1)),
    };
    Ok(Json(play))
}

/// The rate of the last `reset`.
static RATE: AtomicU32 = AtomicU32::new(0);

#[plugin_fn]
pub fn reset(Json(format): Json<Format>) -> FnResult<()> {
    RATE.store(format.rate, Ordering::Relaxed);
    Ok(())
}

#[plugin_fn]
pub fn process(block: Vec<u8>) -> FnResult<Vec<u8>> {
    let mut samples = samples_of(&block);
    match config::get("mode")?.unwrap_or_default().as_str() {
        "gain" => {
            let gain: f32 = config::get("gain")?.unwrap_or_default().parse()?;
            samples.iter_mut().for_each(|s| *s *= gain);
        }
        // Every sample is the rate it was told.
        "rate" => samples.fill(RATE.load(Ordering::Relaxed) as f32),
        "short" => samples.truncate(1),
        "trap" => panic!("a bug in the plug-in"),
        "loop" => loop {
            std::hint::black_box(());
        },
        other => return Err(WithReturnCode::new(Error::msg(format!("no mode {other:?}")), 1)),
    }
    Ok(bytes_of(&samples))
}

/// The answer of a visual plug-in; `fixtures/test-visual` selects it.
#[plugin_fn]
pub fn scene(_: Vec<u8>) -> FnResult<String> {
    Ok(r#"{"data_bytes":4,"passes":[{"shader":"x"}]}"#.to_string())
}

/// `echo` gives back the first four bytes; `hang` never answers.
#[plugin_fn]
pub fn frame(input: Vec<u8>) -> FnResult<Vec<u8>> {
    if config::get("mode")?.unwrap_or_default() == "hang" {
        loop {
            std::hint::black_box(0);
        }
    }
    Ok(input[..4].to_vec())
}

// The decoder plays `.ptest` files: a u32 rate, then mono f32 samples,
// all little-endian. Its `mode` setting says how it misbehaves.

/// Frames of the file, and the next one to give.
static FRAMES: AtomicU64 = AtomicU64::new(0);
static POS: AtomicU64 = AtomicU64::new(0);

fn input(offset: u64, length: u32) -> Result<Vec<u8>, Error> {
    let data = unsafe { popamp_input(Json(Range { offset, length }))? }.0;
    match data.error {
        Some(e) => Err(Error::msg(e)),
        None => Ok(data.data),
    }
}

#[plugin_fn]
pub fn open_file(Json(file): Json<FileInfo>) -> FnResult<Json<Stream>> {
    let head = input(0, 4)?;
    let rate = u32::from_le_bytes(head.try_into().map_err(|_| Error::msg("no rate"))?);
    let frames = file.size.saturating_sub(4) / 4;
    FRAMES.store(frames, Ordering::Relaxed);
    POS.store(0, Ordering::Relaxed);
    let tags = Tags {
        title: Some(format!("A .{} file", file.extension)),
        codec: Some("TEST".into()),
        channels: Some(1),
        sample_rate: Some(rate),
        ..Default::default()
    };
    Ok(Json(Stream { rate, length_ms: Some(frames * 1000 / rate as u64), seekable: true, tags }))
}

#[plugin_fn]
pub fn decode(_: Vec<u8>) -> FnResult<Vec<u8>> {
    match config::get("mode")?.unwrap_or_default().as_str() {
        "trap" => panic!("a bug in the plug-in"),
        "loop" => loop {
            std::hint::black_box(());
        },
        _ => {}
    }
    let pos = POS.load(Ordering::Relaxed);
    let n = FRAMES.load(Ordering::Relaxed).saturating_sub(pos).min(1024);
    let mono = samples_of(&input(4 + pos * 4, n as u32 * 4)?);
    POS.store(pos + mono.len() as u64, Ordering::Relaxed);
    Ok(bytes_of(&mono.iter().flat_map(|&s| [s, s]).collect::<Vec<f32>>()))
}

#[plugin_fn]
pub fn seek(Json(ms): Json<u64>) -> FnResult<Json<u64>> {
    let rate = u32::from_le_bytes(input(0, 4)?.try_into().map_err(|_| Error::msg("no rate"))?) as u64;
    let pos = (ms * rate / 1000).min(FRAMES.load(Ordering::Relaxed));
    POS.store(pos, Ordering::Relaxed);
    Ok(Json(pos * 1000 / rate))
}
