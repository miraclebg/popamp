//! A source plug-in for the host's tests. The path of a location under
//! `https://test.invalid/` says what to do.

use extism_pdk::*;
use popamp_plugin_api::{config as keys, ExecOutput, ExecRequest, Play, Tags, Track, VERSION};

#[host_fn]
extern "ExtismHost" {
    fn popamp_exec(req: Json<ExecRequest>) -> Json<ExecOutput>;
}

fn path(location: &str) -> &str {
    location.strip_prefix("https://test.invalid").unwrap_or(location)
}

fn exec(program: &str, script: &str) -> Result<ExecOutput, Error> {
    let req = ExecRequest { program: program.into(), args: vec!["-c".into(), script.into()] };
    Ok(unsafe { popamp_exec(Json(req))? }.0)
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
        // Asks a program for the rate, then plays 4 frames of silence.
        "/pipe" => {
            let out = exec("sh", "echo 44100")?;
            if let Some(e) = out.error {
                return Err(WithReturnCode::new(Error::msg(e), 1));
            }
            Play::Pipe {
                program: "sh".into(),
                args: vec!["-c".into(), "head -c 32 /dev/zero".into()],
                rate: out.stdout.trim().parse()?,
                seekable: false,
                tags: Tags { title: Some("Piped".into()), ..Default::default() },
            }
        }
        // A program that does not stop is stopped when the call's time is up.
        "/slow" => {
            let out = exec("sh", "sleep 5")?;
            return Err(WithReturnCode::new(Error::msg(out.error.unwrap_or_default()), 1));
        }
        "/forbidden" => {
            exec("rm", "")?;
            unreachable!("the host refuses rm")
        }
        "/trap" => panic!("a bug in the plug-in"),
        "/loop" => loop {
            std::hint::black_box(());
        },
        other => return Err(WithReturnCode::new(Error::msg(format!("no track at {other}")), 1)),
    };
    Ok(Json(play))
}
