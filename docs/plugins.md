# Plug-ins: design

Status: **agreed 2026-11-05**. Steps 1 to 5 of the order of work are
built; the plug-ins are
[plugin-youtube](https://github.com/popamp/plugin-youtube) (`source`)
and [plugin-crossfeed](https://github.com/popamp/plugin-crossfeed)
(`dsp`).

Popamp's core plays local files, HTTP streams and playlists, and draws
skins. Everything else that only some users want, such as YouTube, more
formats or a MilkDrop window, should be a plug-in in a repository of its
own, the same way skins other than the default one are. This document
fixes how plug-ins work before any code is written.

## Goals

- The core stays small. A feature that wraps a third-party service or
  program is never built into it.
- A plug-in cannot crash, hang or slow down the player. A plug-in that
  fails is switched off and the player goes on.
- A plug-in does only what its manifest says it may, and the user sees
  that before turning it on.
- A plug-in built once keeps working across Popamp and compiler updates
  within the same major API version.

Non-goals for version 1: native `.so` plug-ins (Rust has no stable ABI),
plug-ins that draw their own windows, output plug-ins, and loading
Winamp DLLs.

## Kinds

| Kind | Does | Examples | Version |
|---|---|---|---|
| `source` | Takes links it recognises; lists them, describes them and tells the core how to play them | YouTube, SoundCloud, radio directories | 1 |
| `dsp` | Changes decoded samples, block by block | crossfeed, compressor, pitch | 1 |
| `decoder` | Plays local files of a format the core lacks | Monkey's Audio, WavPack, chiptunes | 1.1 |
| `vis` | Draws from the spectrum and wave data into a pixel buffer | spectrum, MilkDrop-style scenes | later |

Version 1.0 had `source` and `dsp`: YouTube needs the first, and the
second tests the hot path; `visual` came next, and `decoder` with 1.1.
A plug-in may be of one kind only.

## Runtime: Extism

A plug-in is a WebAssembly module (`plugin.wasm`) run by
[Extism](https://extism.org), which is built on wasmtime. The core is the
host (the `extism` crate); a plug-in written in Rust uses `extism-pdk`,
and other languages have their own Extism kits.

Extism was chosen over wasmtime's component model because it is quicker
to bring in: it already has call time limits with cancellation, memory
limits, per-plug-in configuration, HTTP limited to listed hosts, and a
simple way to add host functions. What it lacks is a typed interface
file: calls pass bytes. So:

- Calls pass JSON. The types live in one small crate,
  [`popamp-plugin-api`](https://github.com/popamp/popamp-plugin-api), a
  repository of its own, used by the core and by plug-ins written in Rust
  as a git dependency, so both sides are checked by the
  compiler. The crate's `MAJOR.MINOR` is the API version. This document
  lists the calls; the crate is their definition.
- Audio blocks for `dsp` plug-ins pass as raw little-endian `f32`
  samples, not JSON.

Plug-ins are compiled to native code by wasmtime when loaded; the binary
grows by several MB for that compiler.

So plug-in support is a Cargo feature, off by default:

- `cargo build` makes the player and its skins, without wasmtime: no
  "Plug-ins…" menu item, and the plug-ins folder is not read.
- `cargo build --features plugins` adds `popamp-plugins` and with it the
  host, the loading at start-up and the "Plug-ins…" window. That code is
  in `apps/popamp/src/plugins.rs`; the rest of the app marks the few
  places that reach it with `#[cfg(feature = "plugins")]`.
- The source interface (`Sources`) is in both builds; without the
  feature it holds the built-in sources only. CI checks both builds.

## Where plug-ins live

```
~/.local/share/popamp/plugins/<id>/
    plugin.toml      manifest
    plugin.wasm      the module
```

The Gallery window (Options > Gallery) installs and updates the
plug-ins listed on https://popamp.app: it shows what each may do first,
checks the download against the catalog's size and SHA-256, and unpacks
it into a folder named after its id, replacing an older version only
once the new one is whole. On Windows, which has no packages, that is
the way to install one. A plug-in can also be installed by cloning its repository there, like a skin
(hidden files such as `.git` are ignored), or as a package
(`popamp-plugin-<id>` with apt, zypper or dnf), which puts it in
`/usr/share/popamp/plugins/<id>/`; your own folder of the same name wins.
A packaged plug-in, too, stays off until you allow what it asks for.
The core ships no plug-ins.
The menu "Plug-ins…" lists them, turns each on or off, and shows its
settings and permissions.

### Manifest

```toml
id = "youtube"                 # folder name, [a-z0-9-]
name = "YouTube"
version = "1.0.0"
api = "1.0"                    # the API version it was built for
min_popamp = "0.8.0"           # optional: the oldest Popamp it works with
kind = "source"
description = "Plays YouTube videos and playlists through yt-dlp."

[source]
hosts = ["youtube.com", "*.youtube.com", "youtu.be"]
slow = true                    # say "CONNECTING…" while it opens

[permissions]
net = []                       # addresses it may reach over HTTP(S)
# files = "read"               # that it reads files; the user picks the folders

[[permissions.exec]]           # a command it may run (see Security)
id = "resolve"                 # the plug-in asks for it by this name
program = "yt-dlp"             # a name, never a path
args = ["-j", ["--proxy", "{proxy}"], "--", "{url}"]
[permissions.exec.vars]
url = { type = "url", hosts = ["youtube.com", "*.youtube.com", "youtu.be"] }

[[setting]]
key = "yt-dlp"                 # same key as the program: where it is
label = "yt-dlp program"
type = "path"
default = "yt-dlp"

[[setting]]
key = "format"
label = "Format"
type = "text"
default = "bestaudio[ext=m4a]/bestaudio"
```

A `dsp` plug-in has `kind = "dsp"`, no `[source]` table and no
permissions: it runs on every block of sound, so it may not start
programs or reach the network. It may have settings.

A `decoder` has `kind = "decoder"`, `api = "1.1"` or newer, no
permissions, and a `[decoder]` table instead of `[source]`:

```toml
[decoder]
extensions = ["ape", "mac"]    # lowercase, without the dot
```

An extension Popamp plays itself (`flac`, `mid`, `cue` and the rest) is
refused: the built-in decoders always come first, so a plug-in adds
formats and replaces none.

The core checks the manifest strictly, like `skin.toml`: an unknown key
or a missing file keeps the plug-in off, and the error is shown.
Settings are stored in `popamp.ini` under `[plugin.<id>]`; the core draws
their form, so plug-ins draw nothing. A plug-in reads them as its Extism
configuration, by key, together with `popamp.proxy`, the user's proxy.
The permissions the user allowed are stored under `[plugins]`, one line
per plug-in; the plug-in is on while they match what its manifest asks
for.

`[source] hosts` says which locations a source takes: http(s) URLs on
those hosts (`example.com` that host only, `*.example.com` its
subdomains). The core decides this without calling the plug-in, so no
plug-in code runs on the UI or engine thread.

## Security

By default a plug-in can compute and nothing else: no files, no network,
and no clock but the host function `popamp_now`, which returns
the time in milliseconds since 1970 UTC, so a plug-in can tell when what
it keeps has expired. Three permissions widen this, and
the user confirms them when turning the plug-in on, and again when a new
version asks for more:

- **`exec`**: the plug-in may run the commands written in its manifest,
  and no others. The user reads them before allowing the plug-in; any
  change to one in a new version asks again. The plug-in names a command
  and gives the values of its variables; it never supplies arguments.
  Each argument in the manifest is a fixed text or a whole `{variable}`,
  and a list inside `args` is left out as a whole when a variable in it
  has no value. No shell is involved: a value is always one argument.

  | Type | A value must be |
  |---|---|
  | `url` | `https://`, on one of the variable's `hosts`; no user, port or backslash |
  | `text` | one line that is not a path or an address |
  | `number` | digits and at most one point |
  | `headers` | lines of `Name: value`, each ended by CR LF |

  No value may start with `-`, so none can pass for an option. The
  player alone fills in `{proxy}` (the user's proxy, when there is one)
  and `{start}` (the position, in a pipe that can seek). A value that
  breaks these rules, or a variable the command does not have, fails the
  call. A program that runs whatever it is told (a shell, an
  interpreter, `env`, `xargs`, `find`, `sudo` and the like) is refused.
  A setting named after a program says where that program is; its
  default must be the program's own name, so only the user can point it
  elsewhere. The program runs with the user's rights: what it does
  beyond its arguments is trusted, and the dialog says so.
- **`net`**: the plug-in may make HTTP(S) requests through `popamp_http`,
  to the addresses its manifest lists: `example.com`, `*.example.com`
  (no bare `*`), each with `:port` if it needs another port than 80 or
  443. The user can untick any of them. An address with an unusual port
  (`radio.example.com:8000`), and one on this machine or the user's
  network, written out by name (`localhost:8000`, `192.168.1.5:8000`),
  is off until the user ticks it. Any other address cannot lead to this
  machine or the user's network: without a proxy only public addresses
  are connected to; through a proxy, which resolves names itself, a name
  known to be local is refused. Redirects are not followed: the answer
  comes back as it is, and going there is a new request to a listed
  address. Requests use the user's proxy, local ones go direct. Extism's
  own HTTP is shut for every plug-in.
- **`files`**: `files = "read"` says the plug-in reads files, through
  `popamp_list` and `popamp_read`. Paths differ on every system, so the
  manifest names none: the user chooses, at first their music folder,
  or a list of folders, or `~` for the whole home. Links are followed
  before checking, so one inside a chosen folder leads nowhere else.
  Hidden files and folders (`.ssh`, `.gnupg`, `.config`, browser
  profiles), `AppData` and `Library` in the home, and the player's own
  folders stay closed whatever was chosen.

The Plug-ins window shows these above the plug-in's own settings. What
the user unticks or chooses is kept apart from those settings, so no
manifest can set or read it, and being refused is an answer to the
plug-in, not a failure of it.

Every request of these three kinds, allowed or refused, is written to
`.journal/<id>.log` in the user's plug-ins folder with the values the
plug-in gave, never the contents of a file. The Journal button in the
Plug-ins window opens it.

Rejected alternative: plug-ins as separate processes speaking JSON over
standard input and output. They wrap programs such as yt-dlp naturally,
but they are native code with all the user's rights and no sandbox at
all, and SPEC rules native plug-ins out.

## How a source plug-in plays

The plug-in never handles audio. It answers questions, and the core does
the playing:

Calls (JSON in and out; Rust types in `popamp-plugin-api`):

| Call | Input | Output |
|---|---|---|
| `api_version` | none | `"1.0"`, the API it was built for |
| `expand` | a location of a list (a playlist, a channel) | its tracks: location, title, length |
| `describe` | a location | its tags: title, artist, length; or none |
| `open` | a location | how to play it, below |

`open` answers one of:

- `http`: a URL and headers; the core opens it like any HTTP stream;
- `pipe`: a command (one of `exec`), the values of its variables, the
  tags of the track, the sample rate and whether it can seek. The
  program writes interleaved stereo 32-bit float samples, little-endian,
  to standard output; the core starts it and plays that output.
  `{start}` in the command is the position in seconds (`12.345`) each
  time the core starts the program: when the track opens and on every
  seek, so seeking calls no plug-in. A part of the command with
  `{start}` is left out when the track cannot seek.

An error is a string the core shows in the title display.

For YouTube, `open` runs yt-dlp, reads the stream URL and headers from
its JSON and returns a `pipe` that starts ffmpeg on that URL.

## How a dsp plug-in changes the sound

| Call | Input | Output |
|---|---|---|
| `api_version` | none | `"1.0"` |
| `reset` | `{"rate": 44100}` | none |
| `process` | a block, raw bytes | the block changed, as long |

A block is interleaved stereo 32-bit float samples, little-endian, as
the decoder gave them (any length). The core calls `reset` before the
first block, when a track starts, on a seek and when the rate changes;
whatever the plug-in keeps between blocks starts afresh then. The
engine runs dsp plug-ins in folder order, after the equalizer and
before the visualizer, volume and balance. Each one has an instance of
its own, called only by the engine, so it may keep state in statics.

## How a decoder plug-in plays

The engine asks the decoder for the sound of a local file whose
extension it lists, in a folder, a playlist or a CUE sheet alike; the
Open dialog offers those files too. Each file gets an instance of the
plug-in of its own, so it may keep the open file in statics.

| Call | Input | Output |
|---|---|---|
| `api_version` | none | `"1.1"` |
| `open_file` | `{"extension": "ape", "size": 1234}` | the stream, JSON |
| `decode` | none | the next block, raw bytes; none at the end |
| `seek` | the position in ms | the position reached |

The stream says `rate`, and may say `length_ms`, `seekable` and `tags`
(as a source's `describe`, with `channels` and `sample_rate` as the file
has them). A block is interleaved stereo 32-bit float samples,
little-endian, at that rate. Reading a playlist's tags only opens the
file.

A decoder reads its file with `popamp_input` (`{"offset", "length"}` in,
at most 1 MiB of raw bytes out, fewer at the end) and can read nothing
else: the call reads the file of
the instance it came from, and for any other plug-in the call fails.

## Visualizer plug-ins

The code that draws them is the
[`popamp-visual`](https://github.com/popamp/popamp-visual) repository, and
[`plugin-spectrum`](https://github.com/popamp/plugin-spectrum) is a working
example.

Only in a build with the `visualizers` feature; see
[building.md](building.md). Elsewhere a `visual` plug-in loads but has no
menu entry. It is written `kind = "visual"`, may ask for no permissions,
and opens from the Visualization menu. F toggles full screen, Esc leaves
it or closes the window.

While it is full screen and music is playing, the screen saver and the
monitor's sleep stay off. On Linux that asks the desktop through
`org.freedesktop.ScreenSaver`; a desktop without that service blanks as
usual. A paused player lets the monitor sleep.

| Call | Input | Output |
|---|---|---|
| `api_version` | none | `"1.0"` |
| `scene` | none | the scene, JSON |
| `frame` | the frame's numbers, raw bytes | `data_bytes` bytes for the shaders |

`frame` is needed only by a scene with `data_bytes` above 0, and is
called once per frame. Its input is 32-bit little-endian floats: `time`,
`dt`, `width`, `height`, 1024 samples of the wave, then 512 spectrum
levels from 0 (silence) to 1 (full scale).

A plug-in owns its look and the player owns the GPU: the plug-in says
what to draw, and every GPU call is the player's.

```json
{
  "data_bytes": 4,
  "targets": [{ "name": "trail", "scale": 1.0 }],
  "passes": [
    { "shader": "...", "target": "trail", "inputs": ["trail"] },
    { "shader": "...", "inputs": ["trail"] }
  ]
}
```

A pass has `shader` (WGSL with the entry points `vs` and `fs`) and may
have `target` (absent means the window), `inputs`, `vertices` (3),
`instances` (1), `topology` (`triangles`, `lines`, `points`) and `blend`
(`none`, `alpha`, `add`). One pass must draw to the window, and a target
has one pass drawing to it. An input drawn by an earlier pass is this
frame's picture; any other is the last frame's, which is how trails are
made.

Every shader sees these in group 0:

| Binding | Declaration |
|---|---|
| 0 | `var<uniform> frame: Frame`, four floats: `time`, `dt`, `width`, `height` |
| 1 | `var<storage, read> audio: Audio`: `wave: array<f32, 1024>`, `spectrum: array<f32, 512>` |
| 2 | `var<storage, read> data: ...`: the bytes from `frame` |
| 3 | `var s: sampler`, linear |
| 4 on | `var t: texture_2d<f32>`, one per input, in order |

A scene has at most 16 passes, 4 targets, 4 inputs per pass, 256 KiB per
shader, 2,000,000 vertices per frame and 1 MiB of data. Compute shaders
cannot be started.

A shader runs on the graphics card and cannot read files or the network,
and its array accesses are kept inside what it was given. **It can still
keep the card busy**: no check can tell in advance how long a shader
runs. Drawing has a thread of its own, so the player keeps playing; three
frames in a row that fail or take over 2 s close the visualizer, and the
driver's own reset handles a card that stops answering, which may
disturb the whole screen for a moment.

## Threads and time limits

- **No source call runs on the audio callback, the UI thread or the
  engine thread.** `expand`, `describe` and `open` run on a pool of
  worker threads. The engine asks for a track, goes on handling
  commands, and receives the opened source when it is ready. Stop or a
  new track while a call is running drops its result when it arrives.
- This also fixes the core's own HTTP streams, which today connect on
  the engine thread, so Stop does not work while a radio station is
  connecting. It is the first step of the work below and is useful
  without any plug-in.
- `expand`, `describe` and `open` must return within 60 s, programs
  started through `exec` included (a program still running then is
  killed). A plug-in that traps or runs out of time or memory (64 MiB)
  fails that call; three such failures in a row switch it off until the
  user presses OK in "Plug-ins…". The title display says so with the
  last failure, and "Plug-ins…" shows it next to the plug-in. An error the plug-in returns itself
  ("video unavailable") is not a failure.
- A `dsp` plug-in runs on the engine thread, which decodes and fills
  the output buffer (`buffer_ms`), never in a sound card callback. A
  block that fails, comes back with another length or takes longer
  than half its playing time plays unchanged; three in a row switch the
  plug-in off until the user presses OK in "Plug-ins…". A call that
  hangs is stopped after 250 ms.
- A `decoder` runs on the thread that asks: the engine's when it plays,
  a worker's when it reads tags. A call that hangs is stopped after 1 s;
  a call that fails ends the track. Three crashes in a row switch the
  plug-in off; only a block decoded clears them, so a decoder that opens
  every file and crashes on each one stops too.

## Versions

The API version is `MAJOR.MINOR`, returned by the plug-in's `api_version`
call and written in its manifest. Popamp loads plug-ins built for its
major version and the same or an older minor version; minor versions
only add calls or optional fields. A plug-in built for another major
version stays off, with a message naming both versions.

**The API is under development: until Popamp 1.0 it can change with no
compatibility with older plug-ins.** A new kind of plug-in moves the
minor version (1.1 brought `decoder`); other changes are made in place.
Dependents follow the crate's `main` branch, and `Cargo.lock` pins the
commit. A plug-in's package names the oldest Popamp it runs on.

## What left the core

With the YouTube plug-in:

- `crates/popamp-audio/src/youtube.rs` is gone; the core's sources are
  HTTP and files, and plug-ins come before them;
- the settings `ytdlp_path`, `ffmpeg_path` and `ytdlp_format` move to
  `[plugin.youtube]` (`yt-dlp`, `ffmpeg`, `format`) when `popamp.ini` is
  loaded, unless left at their defaults or already set there;
- a link that answers with a web page (`text/html`), such as YouTube with
  the plug-in missing or off, fails with "no plug-in plays this link";
- a build without the `plugins` feature has no YouTube at all.

## Order of work

1. Core: open tracks off the engine thread, for files and HTTP streams.
   Stop works while connecting.
2. Core: the source interface inside the player, with the built-in HTTP
   and file sources behind it (SPEC section 6). Done: the `Source` trait
   and `Playback` in `crates/popamp-audio/src/source.rs`, the pipe player
   in `pipe.rs`.
3. Host: Extism, the `popamp-plugin-api` crate, manifest, permissions dialog, the "Plug-ins…" window,
   settings form. Done: the `popamp-plugin-api` repository, `crates/popamp-plugins`
   (manifest, host, installed plug-ins) and its test plug-in in
   `crates/popamp-plugins/tests/test-plugin`; the "Plug-ins…" window in
   the Options menu shows what each plug-in may do next to its check
   box, so ticking it and pressing OK is the confirmation.
4. The YouTube plug-in in its own repository, `popamp/plugin-youtube`;
   YouTube code leaves the core. Done; the host gives plug-ins the time
   (`popamp_now`) for the plug-in's cache of stream URLs.
5. The `dsp` interface and a small example plug-in. Done: `reset` and
   `process` in the API, the engine's effect chain
   (`popamp_audio::dsp::Effect`), `host::Dsp`, and
   [plugin-crossfeed](https://github.com/popamp/plugin-crossfeed).
6. Visualizers. Done: kind `visual`, the crate
   [popamp-visual](https://github.com/popamp/popamp-visual) and
   [plugin-spectrum](https://github.com/popamp/plugin-spectrum).
7. Decoders. Done: kind `decoder`, API 1.1, `host::Decoding` and
   `popamp_audio::formats::Decoders`.

## Decisions

Agreed with the project owner on 2026-11-05:

1. Extism, not wasmtime's component model directly.
2. `exec` as described: only programs named in the manifest, confirmed by
   the user.
3. Version 1 has `source` and `dsp`; `decoder` comes in version 2.
   Changed on 2026-11-11: `decoder` is API 1.1, since it only adds.
4. Each plug-in, examples included, lives in a repository of its own,
   like the skins; the core keeps only a minimal test plug-in for its
   own tests.
