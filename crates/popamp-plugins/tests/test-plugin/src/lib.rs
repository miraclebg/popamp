//! A plug-in for the host's tests: a source with `fixtures/test`, where
//! the path of a location under `https://test.invalid/` says what to do,
//! and a dsp with `fixtures/test-dsp`, where its `mode` setting does.

use extism_pdk::*;
use popamp_plugin_api::{
    bytes_of, config as keys, samples_of, ExecOutput, ExecRequest, FileData, Format, HttpResponse, Listing, Play, ReadRequest, Tags, Track,
    VERSION,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

#[host_fn]
extern "ExtismHost" {
    fn popamp_exec(req: Json<ExecRequest>) -> Json<ExecOutput>;
    fn popamp_now() -> Json<u64>;
    fn popamp_http(req: Json<popamp_plugin_api::HttpRequest>) -> Json<HttpResponse>;
    fn popamp_list(path: Json<String>) -> Json<Listing>;
    fn popamp_read(req: Json<ReadRequest>) -> Json<FileData>;
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
