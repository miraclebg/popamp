//! Runs one plug-in: its module compiled once, a small pool of instances
//! so two worker threads can call it at once, and the host functions it
//! may use: `popamp_exec`, limited to the programs its manifest names, and
//! `popamp_now`, the time. A dsp plug-in gets an instance of its own for
//! the engine instead: see [`Dsp`].

use crate::manifest::{Kind, Manifest, SourceSpec, MODULE};
use extism::{CompiledPlugin, CurrentPlugin, Function, Pool, PoolBuilder, UserData, Val, PTR};
use popamp_audio::dsp::Effect;
use popamp_audio::pipe::Pipe;
use popamp_audio::source::{Context, Playback, Source};
use popamp_core::meta::TrackMeta;
use popamp_core::plformats::Item;
use popamp_plugin_api::{bytes_of, calls, config, host, samples_of, ExecOutput, ExecRequest, Format, Play, Tags, Track, Version, VERSION};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Told why, when a plug-in stops after failing.
pub type StopNotice = Box<dyn Fn(&str) + Send + Sync>;

/// The longest a call may run, the programs it starts included.
pub const CALL_TIME: Duration = Duration::from_secs(60);
/// The longest a dsp call may run; it holds up the sound, so this is the
/// limit for a plug-in that hangs, not the budget of a block.
pub const DSP_CALL_TIME: Duration = Duration::from_millis(250);
/// Failed calls in a row (traps, timeouts) that switch a plug-in off.
pub const FAILURES_TO_STOP: u32 = 3;
/// Instances per plug-in: calls beyond this many wait for one to be free.
const INSTANCES: usize = 2;
/// Memory per instance, in 64 KiB pages (64 MiB).
const MEMORY_PAGES: u32 = 1024;

pub struct Plugin {
    manifest: Manifest,
    compiled: Arc<CompiledPlugin>,
    pool: Pool,
    programs: Arc<Programs>,
    failures: AtomicU32,
    off: AtomicBool,
    /// The failure that stopped it.
    why: Mutex<Option<String>>,
    notice: OnceLock<StopNotice>,
}

/// The programs a plug-in may start: each name of `exec`, and where it is
/// (the setting of the same key, or the name itself, looked up in `PATH`).
#[derive(Debug, Clone, PartialEq)]
pub struct Programs(Vec<(String, String)>);

impl Programs {
    pub fn new(m: &Manifest, settings: &HashMap<String, String>) -> Programs {
        Programs(
            m.permissions.exec.iter().map(|name| (name.clone(), settings.get(name).cloned().unwrap_or_else(|| name.clone()))).collect(),
        )
    }

    /// Where the program `name` is, if the plug-in may start it.
    pub fn find(&self, name: &str) -> Result<&str, String> {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.as_str())
            .ok_or_else(|| format!("{name} is not in the plug-in's exec permission"))
    }
}

/// The user's values for a plug-in's settings, defaults filled in.
pub fn settings_of(m: &Manifest, values: &HashMap<String, String>) -> HashMap<String, String> {
    m.settings.iter().map(|s| (s.key.clone(), values.get(&s.key).cloned().unwrap_or_else(|| s.default.clone()))).collect()
}

impl std::fmt::Debug for Plugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Plugin({})", self.manifest.id)
    }
}

impl Plugin {
    /// Loads the plug-in in `dir`; `values` are the user's settings for it.
    pub fn load(dir: &Path, values: &HashMap<String, String>, cx: &Context) -> Result<Plugin, String> {
        Plugin::load_with(dir, values, cx, CALL_TIME)
    }

    fn load_with(dir: &Path, values: &HashMap<String, String>, cx: &Context, call_time: Duration) -> Result<Plugin, String> {
        let manifest = Manifest::load(dir)?;
        let call_time = if manifest.kind == Kind::Dsp { call_time.min(DSP_CALL_TIME) } else { call_time };
        let wasm = std::fs::read(dir.join(MODULE)).map_err(|e| format!("{MODULE}: {e}"))?;
        let settings = settings_of(&manifest, values);
        let programs = Arc::new(Programs::new(&manifest, &settings));
        let mut config: Vec<(String, String)> = settings.into_iter().collect();
        config.push((config::PROXY.into(), cx.proxy.clone()));
        let em = extism::Manifest::new([extism::Wasm::data(wasm)])
            .with_timeout(call_time)
            .with_memory_max(MEMORY_PAGES)
            .with_allowed_hosts(manifest.permissions.net.clone().into_iter())
            .with_config(config.into_iter());
        let compiled = extism::PluginBuilder::new(em)
            .with_wasi(false)
            .with_functions([exec_function(programs.clone()), now_function()])
            .compile()
            .map_err(|e| format!("cannot compile {MODULE}: {e}"))?;
        let compiled = Arc::new(compiled);
        let for_pool = compiled.clone();
        let pool = PoolBuilder::new().with_max_instances(INSTANCES).build(move || extism::Plugin::new_from_compiled(&for_pool));
        let plugin = Plugin {
            manifest,
            compiled,
            pool,
            programs,
            failures: AtomicU32::new(0),
            off: AtomicBool::new(false),
            why: Mutex::new(None),
            notice: OnceLock::new(),
        };
        let api: Version = plugin.call(calls::API_VERSION, &())?;
        if !VERSION.loads(api) {
            return Err(format!("built for plug-in API {api}, this Popamp has {VERSION}"));
        }
        Ok(plugin)
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// True when it failed too often and waits for the user to turn it on.
    pub fn is_off(&self) -> bool {
        self.off.load(Ordering::Relaxed)
    }

    pub fn turn_on(&self) {
        self.failures.store(0, Ordering::Relaxed);
        self.off.store(false, Ordering::Relaxed);
        *self.why.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// The failure that stopped it, while it is off.
    pub fn stopped_because(&self) -> Option<String> {
        self.why.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|_| self.is_off())
    }

    /// Calls `notice` with the reason when the plug-in stops; once per stop.
    pub fn when_stopped(&self, notice: StopNotice) {
        let _ = self.notice.set(notice);
    }

    /// Counts a failure; the last of [`FAILURES_TO_STOP`] in a row stops it.
    fn count_failure(&self, why: &str) {
        let n = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        log::warn!("plug-in {}: {why}", self.manifest.id);
        if n >= FAILURES_TO_STOP && !self.off.swap(true, Ordering::Relaxed) {
            log::warn!("plug-in {} switched off after {n} failures", self.manifest.id);
            *self.why.lock().unwrap_or_else(|e| e.into_inner()) = Some(why.to_string());
            if let Some(notice) = self.notice.get() {
                notice(why);
            }
        }
    }

    fn spec(&self) -> Option<&SourceSpec> {
        match &self.manifest.kind {
            Kind::Source(spec) => Some(spec),
            Kind::Dsp => None,
        }
    }

    /// An instance of a dsp plug-in for the engine, which alone calls it.
    pub fn effect(self: &Arc<Self>) -> Result<Dsp, String> {
        let instance = extism::Plugin::new_from_compiled(&self.compiled).map_err(|e| format!("{e:#}"))?;
        Ok(Dsp { plugin: self.clone(), instance, rate: 44_100 })
    }

    /// Calls `name` with `input` as JSON and reads its JSON answer.
    fn call<I: serde::Serialize + ?Sized, O: DeserializeOwned>(&self, name: &str, input: &I) -> Result<O, String> {
        if self.is_off() {
            return Err(format!("plug-in {} is off after failing {FAILURES_TO_STOP} times", self.manifest.id));
        }
        let input = serde_json::to_string(input).map_err(|e| e.to_string())?;
        let Some(mut instance) = self.pool.get(CALL_TIME + CALL_TIME).map_err(|e| format!("{e:#}"))? else {
            return Err(format!("plug-in {} is busy", self.manifest.id));
        };
        let answer = instance.call::<&str, String>(name, &input);
        drop(instance);
        let text = match answer {
            Ok(text) => text,
            Err(e) => return Err(self.failed(&e)),
        };
        self.failures.store(0, Ordering::Relaxed);
        serde_json::from_str(&text).map_err(|e| format!("plug-in {} answered {name} with bad JSON: {e}", self.manifest.id))
    }

    /// Counts a crash or a timeout; an error the plug-in returned is no failure.
    fn failed(&self, e: &extism::Error) -> String {
        let why = e.root_cause().to_string();
        if is_crash(&format!("{e:#}")) {
            self.count_failure(&why);
        } else {
            self.failures.store(0, Ordering::Relaxed);
        }
        why
    }
}

/// True for errors from the runtime (a trap, a timeout, no memory), not
/// for ones the plug-in returned itself.
fn is_crash(error: &str) -> bool {
    error == "timeout" || error == "oom" || error.contains("wasm trap") || error.contains("wasm backtrace")
}

fn exec_function(programs: Arc<Programs>) -> Function {
    Function::new(host::EXEC, [PTR], [PTR], UserData::new(programs), exec)
}

fn now_function() -> Function {
    Function::new(host::NOW, [], [PTR], UserData::new(()), now)
}

/// `popamp_now`: milliseconds since 1970 UTC, as JSON.
fn now(plugin: &mut CurrentPlugin, _: &[Val], outputs: &mut [Val], _: UserData<()>) -> Result<(), extism::Error> {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
    let handle = plugin.memory_new(ms.to_string())?;
    outputs[0] = plugin.memory_to_val(handle);
    Ok(())
}

/// `popamp_exec`: runs a program the manifest names, until it exits or
/// the call's time is up.
fn exec(plugin: &mut CurrentPlugin, inputs: &[Val], outputs: &mut [Val], programs: UserData<Arc<Programs>>) -> Result<(), extism::Error> {
    let text: String = plugin.memory_get_val(&inputs[0])?;
    let req: ExecRequest = serde_json::from_str(&text)?;
    let program = programs
        .get()?
        .lock()
        .map_err(|_| extism::Error::msg("poisoned"))?
        .find(&req.program)
        .map(str::to_string)
        .map_err(extism::Error::msg)?;
    // Stop the program a little before the call's time is up, so the
    // plug-in can still answer that it took too long.
    let left = plugin.time_remaining().unwrap_or(CALL_TIME);
    let deadline = Instant::now() + left.saturating_sub((left / 10).min(Duration::from_secs(1)));
    let out = run(&program, &req.args, deadline).unwrap_or_else(|e| ExecOutput { error: Some(e), ..Default::default() });
    let handle = plugin.memory_new(serde_json::to_string(&out)?)?;
    outputs[0] = plugin.memory_to_val(handle);
    Ok(())
}

/// Runs `program` with standard input closed; kills it at `deadline`.
pub fn run(program: &str, args: &[String], deadline: Instant) -> Result<ExecOutput, String> {
    let mut child =
        Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("{program} not found")
            } else {
                format!("cannot run {program}: {e}")
            }
        })?;
    let read = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let stdout = read(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let stderr = read(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{program} took too long and was stopped"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(ExecOutput {
        error: None,
        status: status.code(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

impl Source for Plugin {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn handles(&self, location: &str) -> bool {
        !self.is_off() && self.spec().is_some_and(|s| s.takes(location))
    }

    fn expand(&self, location: &str, _cx: &Context) -> Result<Option<Vec<Item>>, String> {
        let tracks: Option<Vec<Track>> = self.call(calls::EXPAND, location)?;
        Ok(tracks.map(|t| t.into_iter().map(|t| Item { location: t.location, title: t.title, length_ms: t.length_ms }).collect()))
    }

    fn describe(&self, location: &str) -> Option<Result<TrackMeta, String>> {
        match self.call::<str, Option<Tags>>(calls::DESCRIBE, location) {
            Ok(tags) => tags.map(|t| Ok(meta_of(t))),
            Err(e) => Some(Err(e)),
        }
    }

    fn open(&self, location: &str, _cx: &Context) -> Result<Playback, String> {
        playback_of(self.call(calls::OPEN, location)?, &self.programs)
    }

    fn slow(&self) -> bool {
        self.spec().is_some_and(|s| s.slow)
    }
}

/// A dsp plug-in as the engine runs it. A block that fails, comes back
/// with another length or takes longer than half its playing time plays
/// unchanged, and counts as a failure of the plug-in.
pub struct Dsp {
    plugin: Arc<Plugin>,
    instance: extism::Plugin,
    rate: u32,
}

impl Dsp {
    fn call(&mut self, block: &[f32]) -> Result<Vec<f32>, String> {
        let bytes = bytes_of(block);
        let started = Instant::now();
        let out: Vec<u8> = self.instance.call(calls::PROCESS, bytes.as_slice()).map_err(|e| e.root_cause().to_string())?;
        let budget = Duration::from_secs_f64(block.len() as f64 / 2.0 / self.rate as f64 / 2.0);
        if started.elapsed() > budget {
            return Err(format!("a block took {:?}, more than its {budget:?}", started.elapsed()));
        }
        if out.len() != bytes.len() {
            return Err(format!("a block of {} bytes came back as {}", bytes.len(), out.len()));
        }
        Ok(samples_of(&out))
    }
}

impl Effect for Dsp {
    fn reset(&mut self, rate: u32) {
        self.rate = rate.max(1);
        if self.plugin.is_off() {
            return;
        }
        let format = serde_json::to_string(&Format { rate: self.rate }).unwrap_or_default();
        if let Err(e) = self.instance.call::<&str, ()>(calls::RESET, &format) {
            self.plugin.count_failure(&e.root_cause().to_string());
        }
    }

    fn process(&mut self, block: &mut [f32]) {
        if self.plugin.is_off() || block.is_empty() {
            return;
        }
        match self.call(block) {
            Ok(out) => {
                block.copy_from_slice(&out);
                self.plugin.failures.store(0, Ordering::Relaxed);
            }
            Err(e) => self.plugin.count_failure(&e),
        }
    }
}

/// What the player plays for a plug-in's answer; a pipe may only start a
/// program the manifest names.
pub fn playback_of(play: Play, programs: &Programs) -> Result<Playback, String> {
    Ok(match play {
        Play::Http { url } => Playback::Http(url),
        Play::Pipe { program, args, rate, seekable, tags } => {
            let program = programs.find(&program)?.to_string();
            Playback::Pipe(Box::new(Pipe { program, args, rate, seekable, meta: meta_of(tags) }))
        }
    })
}

pub fn meta_of(t: Tags) -> TrackMeta {
    TrackMeta {
        title: t.title,
        artist: t.artist,
        album: t.album,
        year: t.year,
        genre: t.genre,
        comment: t.comment,
        length_ms: t.length_ms,
        bitrate_kbps: t.bitrate_kbps,
        sample_rate: t.sample_rate,
        channels: t.channels,
        codec: t.codec,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest::parse(
            r#"id = "t"
name = "T"
version = "1"
api = "1.0"
kind = "source"
[source]
hosts = ["t.example"]
[permissions]
exec = ["yt-dlp", "ffmpeg"]
[[setting]]
key = "ffmpeg"
label = "ffmpeg"
type = "path"
default = "/opt/ffmpeg"
"#,
        )
        .unwrap()
    }

    #[test]
    fn a_program_is_found_by_its_setting_or_its_name_and_only_if_listed() {
        let m = manifest();
        let programs = Programs::new(&m, &settings_of(&m, &HashMap::new()));
        assert_eq!(programs.find("ffmpeg"), Ok("/opt/ffmpeg"));
        assert_eq!(programs.find("yt-dlp"), Ok("yt-dlp"));
        assert_eq!(programs.find("sh"), Err("sh is not in the plug-in's exec permission".into()));
        let mine = HashMap::from([("ffmpeg".to_string(), "/usr/bin/ffmpeg".to_string())]);
        assert_eq!(Programs::new(&m, &settings_of(&m, &mine)).find("ffmpeg"), Ok("/usr/bin/ffmpeg"));
    }

    #[test]
    fn a_pipe_may_start_only_a_listed_program() {
        let m = manifest();
        let programs = Programs::new(&m, &settings_of(&m, &HashMap::new()));
        let pipe =
            |program: &str| Play::Pipe { program: program.into(), args: vec![], rate: 48_000, seekable: false, tags: Tags::default() };
        let Playback::Pipe(p) = playback_of(pipe("ffmpeg"), &programs).unwrap() else { panic!() };
        assert_eq!(p.program, "/opt/ffmpeg");
        assert!(playback_of(pipe("rm"), &programs).is_err());
        assert_eq!(playback_of(Play::Http { url: "http://a".into() }, &programs), Ok(Playback::Http("http://a".into())));
    }

    // The program is a shell script.
    #[cfg(unix)]
    #[test]
    fn a_started_program_gives_its_output_and_is_stopped_when_time_is_up() {
        let sh = |script: &str| vec!["-c".to_string(), script.to_string()];
        let later = Instant::now() + Duration::from_secs(5);
        let out = run("sh", &sh("echo out; echo err >&2; exit 3"), later).unwrap();
        assert_eq!(out, ExecOutput { error: None, status: Some(3), stdout: "out\n".into(), stderr: "err\n".into() });
        let started = Instant::now();
        assert_eq!(run("sh", &sh("sleep 5"), started + Duration::from_millis(100)).unwrap_err(), "sh took too long and was stopped");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(run("popamp-no-such-program", &[], later).unwrap_err(), "popamp-no-such-program not found");
    }

    #[test]
    fn only_runtime_errors_count_as_failures() {
        assert!(is_crash("timeout"));
        assert!(is_crash("oom"));
        assert!(is_crash("error while executing at wasm backtrace:\n 0: f\n\nCaused by:\n    wasm trap: unreachable"));
        assert!(!is_crash("video unavailable"));
    }

    /// The plug-in in `tests/test-plugin`, built into `tests/fixtures/test`.
    fn test_plugin(values: &[(&str, &str)], call_time: Duration) -> Plugin {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test");
        let values = values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Plugin::load_with(&dir, &values, &Context { proxy: "proxy:3128".into() }, call_time).unwrap()
    }

    const AT: &str = "https://test.invalid";

    fn open(p: &Plugin, path: &str) -> Result<Playback, String> {
        p.open(&format!("{AT}{path}"), &Context::default())
    }

    #[test]
    fn a_plugin_lists_describes_and_takes_only_its_hosts() {
        let p = test_plugin(&[], CALL_TIME);
        assert_eq!((p.id(), p.slow()), ("test", true));
        assert!(p.handles("https://test.invalid/a"));
        assert!(!p.handles("https://example.com/a"));
        let list = p.expand(&format!("{AT}/list"), &Context::default()).unwrap().unwrap();
        assert_eq!(list[1], Item { location: format!("{AT}/list/2"), title: Some("Track 2".into()), length_ms: Some(2000) });
        assert_eq!(p.expand(&format!("{AT}/one"), &Context::default()), Ok(None));
        assert_eq!(p.describe(&format!("{AT}/a")).unwrap().unwrap().title.as_deref(), Some("Title of /a"));
        assert!(p.describe(&format!("{AT}/untagged")).is_none());
    }

    #[test]
    fn a_plugin_gets_its_settings_and_the_proxy() {
        assert_eq!(open(&test_plugin(&[], CALL_TIME), "/config"), Ok(Playback::Http("http://hello/proxy:3128".into())));
        assert_eq!(open(&test_plugin(&[("greeting", "hi")], CALL_TIME), "/config"), Ok(Playback::Http("http://hi/proxy:3128".into())));
    }

    #[test]
    fn a_plugin_is_told_the_time() {
        let Ok(Playback::Http(url)) = open(&test_plugin(&[], CALL_TIME), "/now") else { panic!() };
        let told: u64 = url.strip_prefix("http://now/").unwrap().parse().unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        assert!(now.abs_diff(told) < 5_000, "told {told}, now {now}");
    }

    /// The dsp side of the test plug-in: `fixtures/test-dsp` with the
    /// module of `fixtures/test`, put together in a folder of its own.
    fn test_dsp(mode: &str) -> Arc<Plugin> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let dir = std::env::temp_dir().join(format!("popamp-dsp-{}-{mode}", std::process::id())).join("test-dsp");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-dsp/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        let values = HashMap::from([("mode".to_string(), mode.to_string())]);
        Arc::new(Plugin::load(&dir, &values, &Context::default()).unwrap())
    }

    /// A block of `frames` stereo frames, left 1.0 and right -1.0.
    fn block(frames: usize) -> Vec<f32> {
        [1.0, -1.0].repeat(frames)
    }

    #[test]
    fn a_dsp_plugin_changes_each_block_and_knows_the_rate() {
        let mut gain = test_dsp("gain").effect().unwrap();
        gain.reset(8_000);
        let mut b = block(4096);
        gain.process(&mut b);
        assert_eq!(b, [0.5, -0.5].repeat(4096));
        let mut rate = test_dsp("rate").effect().unwrap();
        rate.reset(8_000);
        let mut b = block(4096);
        rate.process(&mut b);
        assert!(b.iter().all(|s| *s == 8_000.0), "{:?}", &b[..4]);
    }

    #[test]
    fn a_failed_or_late_dsp_block_plays_unchanged_and_three_in_a_row_switch_it_off() {
        for mode in ["short", "trap", "loop"] {
            let p = test_dsp(mode);
            let mut e = p.effect().unwrap();
            e.reset(8_000);
            for _ in 0..FAILURES_TO_STOP {
                let started = Instant::now();
                let mut b = block(4096);
                e.process(&mut b);
                assert_eq!(b, block(4096), "{mode}");
                assert!(started.elapsed() < Duration::from_secs(1), "{mode}: {:?}", started.elapsed());
            }
            assert!(p.is_off(), "{mode}");
        }
        // 1 frame at 1 MHz has half a microsecond: no call is that quick.
        let p = test_dsp("gain");
        let mut e = p.effect().unwrap();
        e.reset(1_000_000);
        let mut b = block(1);
        e.process(&mut b);
        assert_eq!(b, block(1));
        p.turn_on();
        e.reset(8_000);
        let mut b = block(4096);
        e.process(&mut b);
        assert_eq!(b[0], 0.5, "on again");
    }

    #[test]
    fn a_plugin_that_stops_says_why_once() {
        let p = test_dsp("short");
        let told = Arc::new(std::sync::Mutex::new(Vec::new()));
        let to = told.clone();
        p.when_stopped(Box::new(move |why: &str| to.lock().unwrap().push(why.to_string())));
        assert_eq!(p.stopped_because(), None);
        let mut e = p.effect().unwrap();
        e.reset(8_000);
        for _ in 0..FAILURES_TO_STOP + 2 {
            e.process(&mut block(4096));
        }
        let why = "a block of 32768 bytes came back as 4";
        assert_eq!(p.stopped_because().as_deref(), Some(why));
        assert_eq!(*told.lock().unwrap(), [why]);
    }

    // The program is a shell script.
    #[cfg(unix)]
    #[test]
    fn a_plugin_runs_only_the_programs_it_lists() {
        let p = test_plugin(&[], CALL_TIME);
        let Ok(Playback::Pipe(pipe)) = open(&p, "/pipe") else { panic!("{:?}", open(&p, "/pipe")) };
        assert_eq!((pipe.program.as_str(), pipe.rate, pipe.meta.title.as_deref()), ("sh", 44_100, Some("Piped")));
        assert_eq!(open(&p, "/forbidden"), Err("rm is not in the plug-in's exec permission".into()));
        let p = test_plugin(&[], Duration::from_millis(500));
        assert_eq!(open(&p, "/slow"), Err("sh took too long and was stopped".into()));
        assert!(!p.is_off());
    }

    #[test]
    fn a_plugin_that_keeps_crashing_is_switched_off_and_its_own_errors_do_not_count() {
        let p = test_plugin(&[], Duration::from_millis(300));
        for _ in 0..5 {
            assert_eq!(open(&p, "/missing"), Err("no track at /missing".into()));
        }
        assert!(!p.is_off());
        assert!(open(&p, "/trap").is_err());
        assert!(open(&p, "/loop").unwrap_err().contains("timeout"));
        assert!(!p.is_off(), "two failures");
        assert!(open(&p, "/trap").is_err());
        assert!(p.is_off());
        assert!(!p.handles(&format!("{AT}/a")));
        assert!(open(&p, "/config").unwrap_err().contains("is off"));
        p.turn_on();
        assert!(open(&p, "/config").is_ok());
    }
}
