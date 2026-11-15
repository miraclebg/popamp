//! Runs one plug-in: its module compiled once, a small pool of instances
//! so two worker threads can call it at once, and the host functions it
//! may use: `popamp_exec`, limited to the commands its manifest writes,
//! `popamp_http` and the two that read files, limited to what the manifest
//! asks and the user left open, `popamp_input`, the file a decoder plays,
//! and `popamp_now`, the time. A dsp plug-in gets an instance of its own
//! for the engine instead, see [`Dsp`], and so does each file a decoder
//! opens, see [`Decoding`].

use crate::command::{self, Command, Vars};
use crate::files::{Closed, Folders};
use crate::journal::Journal;
use crate::manifest::{Kind, Manifest, SourceSpec, MODULE};
use crate::net::Network;
use extism::{CompiledPlugin, CurrentPlugin, Function, Pool, PoolBuilder, UserData, Val, PTR};
use popamp_audio::decoder::{AudioSource, Status, StreamInfo};
use popamp_audio::dsp::Effect;
use popamp_audio::formats::FileDecoder;
use popamp_audio::pipe::{Pipe, START};
use popamp_audio::source::{Context, Playback, Source};
use popamp_core::meta::TrackMeta;
use popamp_core::plformats::Item;
use popamp_plugin_api::{
    bytes_of, calls, config, host, samples_of, ExecOutput, ExecRequest, FileInfo, Format, HttpRequest, Play, Range, ReadRequest, Stream,
    Tags, Track, Version, VERSION,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Stdio;
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
/// The longest a decoder call may run: the engine waits for it.
pub const DECODER_CALL_TIME: Duration = Duration::from_secs(1);
/// The most [`host::INPUT`] gives at once.
const INPUT_PIECE: u32 = 1 << 20;
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

/// What the user decided about a plug-in, past what its manifest asks.
#[derive(Debug, Default)]
pub struct Access {
    pub journal: Arc<Journal>,
    /// The addresses of the manifest that are open.
    pub hosts: Vec<String>,
    /// The folders it may read; see [`Closed`] for what stays shut.
    pub folders: Vec<PathBuf>,
    pub closed: Closed,
}

/// The commands a plug-in may run, and what the player fills in itself.
#[derive(Debug)]
pub struct Programs {
    commands: Vec<Command>,
    /// Where a program is, when the user's setting of its name says.
    paths: HashMap<String, String>,
    /// The user's proxy, as a URL.
    proxy: Option<String>,
    journal: Arc<Journal>,
}

impl Programs {
    pub fn new(m: &Manifest, settings: &HashMap<String, String>, proxy: &str, journal: Arc<Journal>) -> Programs {
        let proxy = proxy.trim();
        Programs {
            commands: m.permissions.exec.clone(),
            paths: settings.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect(),
            proxy: (!proxy.is_empty()).then(|| format!("http://{proxy}")),
            journal,
        }
    }

    /// The program and arguments for a plug-in's request, if the manifest
    /// allows exactly that; written in the journal either way.
    pub fn prepare(&self, id: &str, vars: &Vars, seekable: bool) -> Result<(String, Vec<String>), String> {
        let asked = request_text(id, vars);
        let prepared = self.expand(id, vars, seekable);
        match &prepared {
            Ok(_) => self.journal.record("exec", &asked, "allowed"),
            Err(why) => self.journal.record("exec", &format!("{asked} ({why})"), "refused"),
        }
        prepared
    }

    fn expand(&self, id: &str, vars: &Vars, seekable: bool) -> Result<(String, Vec<String>), String> {
        let command = self.commands.iter().find(|c| c.id == id).ok_or_else(|| format!("{id} is not a command of this plug-in"))?;
        let mut built_in = Vars::new();
        if let Some(proxy) = &self.proxy {
            built_in.insert(command::PROXY.into(), proxy.clone());
        }
        if seekable {
            built_in.insert(command::START.into(), START.into());
        }
        let args = command.expand(vars, &built_in)?;
        Ok((self.paths.get(&command.program).unwrap_or(&command.program).clone(), args))
    }
}

/// A request as the journal shows it: what the plug-in said, nothing the
/// player added (the proxy may hold a password).
fn request_text(id: &str, vars: &Vars) -> String {
    let values: Vec<String> = vars.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{id} {}", values.join(" ")).trim_end().to_string()
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
    pub fn load(dir: &Path, values: &HashMap<String, String>, cx: &Context, access: Access) -> Result<Plugin, String> {
        Plugin::load_with(dir, values, cx, access, CALL_TIME)
    }

    fn load_with(
        dir: &Path,
        values: &HashMap<String, String>,
        cx: &Context,
        access: Access,
        call_time: Duration,
    ) -> Result<Plugin, String> {
        let manifest = Manifest::load(dir)?;
        let call_time = match manifest.kind {
            Kind::Source(_) => call_time,
            Kind::Decoder(_) => call_time.min(DECODER_CALL_TIME),
            Kind::Dsp | Kind::Visual => call_time.min(DSP_CALL_TIME),
        };
        let wasm = std::fs::read(dir.join(MODULE)).map_err(|e| format!("{MODULE}: {e}"))?;
        let settings = settings_of(&manifest, values);
        let programs = Arc::new(Programs::new(&manifest, &settings, &cx.proxy, access.journal.clone()));
        let network = Network::new(&access.hosts, &cx.proxy, access.journal.clone());
        let folders = Arc::new(Folders::new(&access.folders, access.closed, access.journal));
        let mut config: Vec<(String, String)> = settings.into_iter().collect();
        config.push((config::PROXY.into(), cx.proxy.clone()));
        let em = extism::Manifest::new([extism::Wasm::data(wasm)])
            .with_timeout(call_time)
            .with_memory_max(MEMORY_PAGES)
            .with_config(config.into_iter());
        let compiled = extism::PluginBuilder::new(em)
            .with_wasi(false)
            .with_functions([
                exec_function(programs.clone()),
                now_function(),
                function(host::HTTP, move |req: HttpRequest, left| network.fetch(req, left)),
                function(host::LIST, {
                    let folders = folders.clone();
                    move |path: String, _| folders.list(&path)
                }),
                function(host::READ, move |req: ReadRequest, _| folders.read(&req)),
                Function::new(host::INPUT, [PTR], [PTR], UserData::new(()), input),
            ])
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
            Kind::Dsp | Kind::Visual | Kind::Decoder(_) => None,
        }
    }

    /// An instance of a dsp plug-in for the engine, which alone calls it.
    pub fn effect(self: &Arc<Self>) -> Result<Dsp, String> {
        let instance = extism::Plugin::new_from_compiled(&self.compiled).map_err(|e| format!("{e:#}"))?;
        Ok(Dsp { plugin: self.clone(), instance, rate: 44_100 })
    }

    /// An instance of a visual plug-in for the thread that draws it.
    pub fn visual(self: &Arc<Self>) -> Result<Visual, String> {
        let instance = extism::Plugin::new_from_compiled(&self.compiled).map_err(|e| format!("{e:#}"))?;
        Ok(Visual { plugin: self.clone(), instance })
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

/// A host function that takes JSON and answers JSON, given the time the
/// call has left. Whatever goes wrong is said in the answer: being refused
/// what the user unticked is no failure of the plug-in.
fn function<I, O>(name: &str, answer: impl Fn(I, Duration) -> O + Send + Sync + 'static) -> Function
where
    I: DeserializeOwned,
    O: Serialize,
{
    let call = move |plugin: &mut CurrentPlugin, inputs: &[Val], outputs: &mut [Val], _: UserData<()>| {
        let text: String = plugin.memory_get_val(&inputs[0])?;
        let left = plugin.time_remaining().unwrap_or(CALL_TIME);
        let out = answer(serde_json::from_str(&text)?, left);
        let handle = plugin.memory_new(serde_json::to_string(&out)?)?;
        outputs[0] = plugin.memory_to_val(handle);
        Ok(())
    };
    Function::new(name, [PTR], [PTR], UserData::new(()), call)
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

/// `popamp_exec`: runs a command the manifest writes, until it exits or
/// the call's time is up.
fn exec(plugin: &mut CurrentPlugin, inputs: &[Val], outputs: &mut [Val], programs: UserData<Arc<Programs>>) -> Result<(), extism::Error> {
    let text: String = plugin.memory_get_val(&inputs[0])?;
    let req: ExecRequest = serde_json::from_str(&text)?;
    let (program, args) = programs
        .get()?
        .lock()
        .map_err(|_| extism::Error::msg("poisoned"))?
        .prepare(&req.command, &req.vars, false)
        .map_err(extism::Error::msg)?;
    // Stop the program a little before the call's time is up, so the
    // plug-in can still answer that it took too long.
    let left = plugin.time_remaining().unwrap_or(CALL_TIME);
    let deadline = Instant::now() + left.saturating_sub((left / 10).min(Duration::from_secs(1)));
    let out = run(&program, &args, deadline).unwrap_or_else(|e| ExecOutput { error: Some(e), ..Default::default() });
    let handle = plugin.memory_new(serde_json::to_string(&out)?)?;
    outputs[0] = plugin.memory_to_val(handle);
    Ok(())
}

/// Runs `program` with standard input closed; kills it at `deadline`.
pub fn run(program: &str, args: &[String], deadline: Instant) -> Result<ExecOutput, String> {
    let mut child =
        std::process::Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(
            |e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    format!("{program} not found")
                } else {
                    format!("cannot run {program}: {e}")
                }
            },
        )?;
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

/// A visual plug-in as the drawing thread runs it. `scene` answers what
/// to draw, as JSON. `frame`, needed only by a scene with `data_bytes`,
/// gets the frame's numbers and answers the bytes its shaders read.
pub struct Visual {
    plugin: Arc<Plugin>,
    instance: extism::Plugin,
}

impl Visual {
    pub fn scene(&mut self) -> Result<Vec<u8>, String> {
        self.call(calls::SCENE, &[])
    }

    pub fn frame(&mut self, input: &[u8]) -> Result<Vec<u8>, String> {
        self.call(calls::FRAME, input)
    }

    fn call(&mut self, name: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        if self.plugin.is_off() {
            return Err(format!("plug-in {} is off after failing {FAILURES_TO_STOP} times", self.plugin.manifest.id));
        }
        match self.instance.call::<&[u8], Vec<u8>>(name, input) {
            Ok(out) => {
                self.plugin.failures.store(0, Ordering::Relaxed);
                Ok(out)
            }
            Err(e) => Err(self.plugin.failed(&e)),
        }
    }
}

thread_local! {
    /// The file the decoder called on this thread plays: the call runs on
    /// the caller's thread, so `popamp_input` finds it here.
    static INPUT: RefCell<Option<File>> = const { RefCell::new(None) };
}

/// Runs `call` with `file` readable through `popamp_input`.
fn with_input<T>(file: &mut Option<File>, call: impl FnOnce() -> T) -> T {
    INPUT.with(|i| *i.borrow_mut() = file.take());
    let out = call();
    *file = INPUT.with(|i| i.borrow_mut().take());
    out
}

/// `popamp_input`: a piece of the file being decoded on this thread, as
/// raw bytes; a decoder reads its whole file this way.
fn input(plugin: &mut CurrentPlugin, inputs: &[Val], outputs: &mut [Val], _: UserData<()>) -> Result<(), extism::Error> {
    let range: Range = serde_json::from_str(&plugin.memory_get_val::<String>(&inputs[0])?)?;
    let data = INPUT.with(|i| {
        let mut file = i.borrow_mut();
        let file = file.as_mut().ok_or_else(|| extism::Error::msg("only a decoder reads its file"))?;
        let mut data = Vec::new();
        file.seek(SeekFrom::Start(range.offset))?;
        file.take(range.length.min(INPUT_PIECE) as u64).read_to_end(&mut data)?;
        Ok::<_, extism::Error>(data)
    })?;
    let handle = plugin.memory_new(data)?;
    outputs[0] = plugin.memory_to_val(handle);
    Ok(())
}

/// A decoder plug-in as the player's format dispatch sees it.
pub struct DecoderPlugin(pub Arc<Plugin>);

impl FileDecoder for DecoderPlugin {
    fn extensions(&self) -> &[String] {
        match &self.0.manifest.kind {
            Kind::Decoder(spec) => &spec.extensions,
            _ => &[],
        }
    }

    fn open(&self, path: &Path) -> Result<(Box<dyn AudioSource>, TrackMeta), String> {
        let plugin = &self.0;
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let instance = extism::Plugin::new_from_compiled(&plugin.compiled).map_err(|e| format!("{e:#}"))?;
        let mut decoding = Decoding { plugin: plugin.clone(), instance, file: Some(file), info: StreamInfo::default() };
        let stream: Stream = decoding.call_json(calls::OPEN_FILE, &FileInfo { extension: extension.clone(), size })?;
        if !(1..=768_000).contains(&stream.rate) {
            return Err(format!("plug-in {} gave the rate {}", plugin.manifest.id, stream.rate));
        }
        let mut meta = meta_of(stream.tags);
        meta.length_ms = meta.length_ms.or(stream.length_ms);
        meta.sample_rate = meta.sample_rate.or(Some(stream.rate));
        meta.codec = meta.codec.or_else(|| Some(extension.to_uppercase()));
        decoding.info = StreamInfo {
            sample_rate: stream.rate,
            channels: meta.channels.unwrap_or(2),
            length_ms: stream.length_ms,
            bitrate_kbps: meta.bitrate_kbps,
            codec: meta.codec.clone().unwrap_or_default(),
            seekable: stream.seekable,
        };
        Ok((Box::new(decoding), meta))
    }
}

/// A file a decoder plug-in plays, with an instance of its own. A call
/// that fails ends the track; three crashes in a row switch it off.
pub struct Decoding {
    plugin: Arc<Plugin>,
    instance: extism::Plugin,
    file: Option<File>,
    info: StreamInfo,
}

impl Decoding {
    fn call_json<I: Serialize, O: DeserializeOwned>(&mut self, name: &str, input: &I) -> Result<O, String> {
        let out = self.call(name, serde_json::to_string(input).map_err(|e| e.to_string())?.as_bytes())?;
        serde_json::from_slice(&out).map_err(|e| format!("plug-in {} answered {name} with bad JSON: {e}", self.plugin.manifest.id))
    }

    fn call(&mut self, name: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        if self.plugin.is_off() {
            return Err(format!("plug-in {} is off after failing {FAILURES_TO_STOP} times", self.plugin.manifest.id));
        }
        let instance = &mut self.instance;
        // Only a block decoded clears the failures: a plug-in that opens
        // every file and crashes on each would never stop otherwise.
        with_input(&mut self.file, || instance.call::<&[u8], Vec<u8>>(name, input)).map_err(|e| self.plugin.failed(&e))
    }
}

impl AudioSource for Decoding {
    fn info(&self) -> &StreamInfo {
        &self.info
    }

    fn next(&mut self, out: &mut Vec<f32>) -> Result<Status, String> {
        let block = self.call(calls::DECODE, &[])?;
        if block.len() % 8 != 0 {
            return Err(format!("plug-in {} gave a block of {} bytes, not whole stereo frames", self.plugin.manifest.id, block.len()));
        }
        self.plugin.failures.store(0, Ordering::Relaxed);
        out.extend(samples_of(&block));
        Ok(if block.is_empty() { Status::Eof } else { Status::Data })
    }

    fn seek(&mut self, ms: u64) -> Result<u64, String> {
        self.call_json(calls::SEEK, &ms)
    }
}

/// command the manifest writes.
pub fn playback_of(play: Play, programs: &Programs) -> Result<Playback, String> {
    Ok(match play {
        Play::Http { url } => Playback::Http(url),
        Play::Pipe { command, vars, rate, seekable, tags } => {
            let (program, args) = programs.prepare(&command, &vars, seekable)?;
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
[[permissions.exec]]
id = "get"
program = "yt-dlp"
args = [["--proxy", "{proxy}"], "{word}"]
[permissions.exec.vars]
word = { type = "text" }
[[permissions.exec]]
id = "play"
program = "ffmpeg"
args = [["-ss", "{start}"], "pipe:1"]
[[setting]]
key = "ffmpeg"
label = "ffmpeg"
type = "path"
default = "ffmpeg"
"#,
        )
        .unwrap()
    }

    fn programs(user: &[(&str, &str)], proxy: &str, journal: Journal) -> Programs {
        let m = manifest();
        let user = user.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Programs::new(&m, &settings_of(&m, &user), proxy, Arc::new(journal))
    }

    fn word(w: &str) -> Vars {
        Vars::from([("word".to_string(), w.to_string())])
    }

    #[test]
    fn a_program_is_where_the_user_says_or_found_by_its_name() {
        assert_eq!(programs(&[], "", Journal::default()).prepare("play", &Vars::new(), false).unwrap().0, "ffmpeg");
        let mine = programs(&[("ffmpeg", "/usr/bin/ffmpeg")], "", Journal::default());
        assert_eq!(mine.prepare("play", &Vars::new(), false).unwrap().0, "/usr/bin/ffmpeg");
        let cleared = programs(&[("ffmpeg", " ")], "", Journal::default());
        assert_eq!(cleared.prepare("play", &Vars::new(), false).unwrap().0, "ffmpeg");
    }

    #[test]
    fn the_player_alone_fills_in_the_proxy_and_the_start() {
        let p = programs(&[], " u:p@h:1 ", Journal::default());
        assert_eq!(p.prepare("get", &word("a"), false).unwrap().1, ["--proxy", "http://u:p@h:1", "a"]);
        assert_eq!(programs(&[], "", Journal::default()).prepare("get", &word("a"), false).unwrap().1, ["a"]);
        assert_eq!(p.prepare("play", &Vars::new(), true).unwrap().1, ["-ss", START, "pipe:1"]);
        assert_eq!(p.prepare("play", &Vars::new(), false).unwrap().1, ["pipe:1"]);
    }

    #[test]
    fn every_request_is_in_the_journal_and_the_proxy_is_not() {
        let dir = std::env::temp_dir().join(format!("popamp-host-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = programs(&[], "secret@h:1", Journal::new(&dir, "t"));
        p.prepare("get", &word("hello"), false).unwrap();
        assert!(p.prepare("rm", &Vars::new(), false).is_err());
        assert!(p.prepare("get", &word("--exec=x"), false).is_err());
        let text = std::fs::read_to_string(Journal::path_in(&dir, "t")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(lines[0].ends_with("exec allowed: get word=hello"), "{text}");
        assert!(lines[1].ends_with("exec refused: rm (rm is not a command of this plug-in)"), "{text}");
        assert!(lines[2].ends_with("exec refused: get word=--exec=x (word: may not start with -)"), "{text}");
        assert!(!text.contains("secret"), "{text}");
    }

    #[test]
    fn a_pipe_may_run_only_a_command_of_the_manifest() {
        let programs = programs(&[("ffmpeg", "/opt/ffmpeg")], "", Journal::default());
        let pipe =
            |command: &str| Play::Pipe { command: command.into(), vars: Vars::new(), rate: 48_000, seekable: false, tags: Tags::default() };
        let Playback::Pipe(p) = playback_of(pipe("play"), &programs).unwrap() else { panic!() };
        assert_eq!((p.program.as_str(), p.args.as_slice()), ("/opt/ffmpeg", ["pipe:1".to_string()].as_slice()));
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
        Plugin::load_with(&dir, &values, &Context { proxy: "proxy:3128".into(), ..Context::default() }, Access::default(), call_time)
            .unwrap()
    }

    fn with_access(access: Access) -> Plugin {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test");
        Plugin::load_with(&dir, &HashMap::new(), &Context::default(), access, CALL_TIME).unwrap()
    }

    #[test]
    fn a_plugin_reads_the_folder_it_was_given_and_not_others() {
        let dir = std::env::temp_dir().join(format!("popamp-host-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("song.txt"), "la la").unwrap();
        let given = with_access(Access { folders: vec![dir.clone()], ..Default::default() });
        assert_eq!(open(&given, "/files"), Ok(Playback::Http("http://files/la la".into())));
        let none = with_access(Access::default());
        assert_eq!(open(&none, "/files").unwrap_err(), "no folders");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_refused_request_is_an_answer_and_not_a_failure_of_the_plugin() {
        let p = with_access(Access::default());
        for _ in 0..FAILURES_TO_STOP + 1 {
            assert_eq!(open(&p, "/net-refused").unwrap_err(), "host not-listed.invalid is not allowed");
        }
        assert!(!p.is_off());
    }

    #[test]
    fn a_host_the_user_unticked_is_closed_like_one_never_listed() {
        let p = with_access(Access { hosts: vec!["other.invalid".into()], ..Default::default() });
        assert_eq!(open(&p, "/net-listed").unwrap_err(), "host test.invalid is not allowed");
    }

    #[test]
    fn the_http_of_the_runtime_is_shut_even_for_a_listed_host() {
        let err = open(&with_access(Access::default()), "/extism-http").unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
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
        // A folder per call: tests run in parallel, and two of them copying
        // into one folder let the other read a half-written file.
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("popamp-dsp-{}-{mode}-{n}", std::process::id())).join("test-dsp");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-dsp/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        let values = HashMap::from([("mode".to_string(), mode.to_string())]);
        Arc::new(Plugin::load(&dir, &values, &Context::default(), Access::default()).unwrap())
    }

    fn test_visual(mode: &str) -> Visual {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let dir = std::env::temp_dir().join(format!("popamp-visual-{}-{mode}", std::process::id())).join("test-visual");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-visual/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        let values = HashMap::from([("mode".to_string(), mode.to_string())]);
        Arc::new(Plugin::load(&dir, &values, &Context::default(), Access::default()).unwrap()).visual().unwrap()
    }

    /// The decoder side of the test plug-in, in a folder per call as `test_dsp`.
    fn test_decoder(mode: &str) -> Arc<Plugin> {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("popamp-decoder-{}-{mode}-{n}", std::process::id())).join("test-decoder");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(fixtures.join("test-decoder/plugin.toml"), dir.join("plugin.toml")).unwrap();
        std::fs::copy(fixtures.join("test/plugin.wasm"), dir.join("plugin.wasm")).unwrap();
        let values = HashMap::from([("mode".to_string(), mode.to_string())]);
        Arc::new(Plugin::load(&dir, &values, &Context::default(), Access::default()).unwrap())
    }

    /// A `.ptest` file: 2.5 s at 8 kHz, each sample its frame number / 1e5.
    fn ptest_file(name: &str) -> PathBuf {
        let mut bytes = 8_000u32.to_le_bytes().to_vec();
        bytes.extend((0..20_000).flat_map(|i| (i as f32 / 1e5).to_le_bytes()));
        let path = std::env::temp_dir().join(format!("popamp-{}-{name}.ptest", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn decode_all(source: &mut dyn AudioSource) -> Vec<f32> {
        let mut out = Vec::new();
        while let Status::Data = source.next(&mut out).unwrap() {}
        out
    }

    #[test]
    fn a_decoder_plays_the_file_it_was_given_from_the_start_or_a_seek() {
        let path = ptest_file("plays");
        let decoder = DecoderPlugin(test_decoder("play"));
        assert_eq!(decoder.extensions(), ["ptest"]);
        let (mut source, meta) = decoder.open(&path).unwrap();
        assert_eq!((meta.title.as_deref(), meta.codec.as_deref(), meta.channels), (Some("A .ptest file"), Some("TEST"), Some(1)));
        assert_eq!((meta.length_ms, meta.sample_rate, source.info().sample_rate), (Some(2_500), Some(8_000), 8_000));
        let out = decode_all(source.as_mut());
        assert_eq!(out.len(), 40_000, "every frame, in stereo");
        assert_eq!((out[2 * 12_345], out[2 * 12_345 + 1]), (0.12345, 0.12345));
        assert_eq!(source.seek(1_000).unwrap(), 1_000);
        let rest = decode_all(source.as_mut());
        assert_eq!((rest.len(), rest[0]), (24_000, 0.08));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_decoder_takes_its_formats_through_the_players_dispatch() {
        let path = ptest_file("dispatch");
        let decoders = popamp_audio::formats::Decoders::new(vec![Arc::new(DecoderPlugin(test_decoder("play")))]);
        assert!(decoders.plays(&path.to_string_lossy()));
        assert_eq!(popamp_audio::read_meta(&path, &decoders).unwrap().codec.as_deref(), Some("TEST"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_decoder_that_crashes_ends_its_track_and_three_in_a_row_switch_it_off() {
        let path = ptest_file("trap");
        let decoder = DecoderPlugin(test_decoder("trap"));
        for _ in 0..FAILURES_TO_STOP {
            let (mut source, _) = decoder.open(&path).unwrap();
            assert!(source.next(&mut Vec::new()).is_err());
        }
        assert!(decoder.0.is_off());
        assert!(decoder.open(&path).err().unwrap().contains("is off"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn only_a_decoder_reads_a_file_through_input() {
        assert_eq!(open(&with_access(Access::default()), "/input").unwrap_err(), "only a decoder reads its file");
    }

    #[test]
    fn a_visual_plugin_answers_its_scene_and_frames() {
        let mut v = test_visual("echo");
        let scene: serde_json::Value = serde_json::from_slice(&v.scene().unwrap()).unwrap();
        assert_eq!(scene["data_bytes"], 4);
        assert_eq!(v.frame(&[1, 2, 3, 4, 5, 6]).unwrap(), [1, 2, 3, 4]);
    }

    #[test]
    fn a_visual_plugin_that_hangs_three_times_is_switched_off() {
        let mut v = test_visual("hang");
        for _ in 0..3 {
            assert_eq!(v.frame(&[0; 4]).unwrap_err(), "timeout");
        }
        assert!(v.frame(&[0; 4]).unwrap_err().contains("is off after failing 3 times"));
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

    // The commands are echo, head and sleep.
    #[cfg(unix)]
    #[test]
    fn a_plugin_runs_only_the_commands_it_wrote_with_values_of_their_types() {
        let p = test_plugin(&[], CALL_TIME);
        let Ok(Playback::Pipe(pipe)) = open(&p, "/pipe") else { panic!("{:?}", open(&p, "/pipe")) };
        assert_eq!((pipe.program.as_str(), pipe.rate, pipe.meta.title.as_deref()), ("head", 44_100, Some("Piped")));
        assert_eq!(pipe.args, ["-c", "32", "/dev/zero"]);
        assert_eq!(open(&p, "/forbidden"), Err("rm is not a command of this plug-in".into()));
        assert_eq!(open(&p, "/option"), Err("rate: may not start with -".into()));
        assert_eq!(open(&p, "/extra"), Err("\"more\" is not a variable of command \"rate\"".into()));
        // A tenth of the call, 200 ms, is left to stop the program and
        // answer; slow CI machines (macOS) need more than 50 ms of it.
        let p = test_plugin(&[], Duration::from_secs(2));
        assert_eq!(open(&p, "/slow"), Err("sleep took too long and was stopped".into()));
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
