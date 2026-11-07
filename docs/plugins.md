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
| `decoder` | Turns bytes of a format the core lacks into samples | Opus, tracker modules, MIDI | 2 |
| `vis` | Draws from the spectrum and wave data into a pixel buffer | MilkDrop through projectM | later |

Version 1 has `source` and `dsp` only: YouTube needs the first, and the
second tests the hot path. A plug-in may be of one kind only.

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

A plug-in can be installed by cloning its repository there, like a skin
(hidden files such as `.git` are ignored), or as a package
(`apt install popamp-plugin-<id>`), which puts it in
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
kind = "source"
description = "Plays YouTube videos and playlists through yt-dlp."

[source]
hosts = ["youtube.com", "*.youtube.com", "youtu.be"]
slow = true                    # say "CONNECTING…" while it opens

[permissions]
exec = ["yt-dlp", "ffmpeg"]    # programs it may start (see Security)
net = []                       # hosts it may reach over HTTP

[[setting]]
key = "yt-dlp"                 # same key as in exec: where that program is
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
it keeps has expired. Two permissions widen this, and
the user confirms them when turning the plug-in on, and again when a new
version asks for more:

- **`exec`**: the plug-in may start the programs named in its manifest,
  and no others, through a host function. The program is found the way
  the setting of the same name says (a path, or a name looked up in
  `PATH`). The host gives the plug-in the program's standard output and
  exit status; standard input is closed. A started program runs with the
  user's rights, so this permission trusts that program, not the
  plug-in: the dialog says so.
- **`net`**: the plug-in may make HTTP(S) requests through Extism's HTTP
  host function, limited to the hosts listed in the manifest
  (`net = ["*.example.com"]`). Extism's HTTP does not go through the
  user's proxy yet; a plug-in that needs it reads `popamp.proxy`.

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
- `pipe`: a program (one of `exec`), its arguments, the tags of the
  track, the sample rate and whether it can seek. The program writes
  interleaved stereo 32-bit float samples, little-endian, to standard
  output; the core starts it and plays that output. `{start}` in an
  argument is replaced by the position in seconds (`12.345`) each time
  the core starts the program: when the track opens and on every seek,
  so seeking calls no plug-in.

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

## Versions

The API version is `MAJOR.MINOR`, returned by the plug-in's `api_version`
call and written in its manifest. Popamp loads plug-ins built for its
major version and the same or an older minor version; minor versions
only add calls or optional fields. A plug-in built for another major
version stays off, with a message naming both versions.

Until the first release (the one that ships packages) the API stays
1.0 and changes in place: dependents follow the crate's `main` branch,
and `Cargo.lock` pins the commit. Versions start to move with that
release.

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
6. Later: `decoder` (version 2), `vis`.

## Decisions

Agreed with the project owner on 2026-11-05:

1. Extism, not wasmtime's component model directly.
2. `exec` as described: only programs named in the manifest, confirmed by
   the user.
3. Version 1 has `source` and `dsp`; `decoder` comes in version 2.
4. Each plug-in, examples included, lives in a repository of its own,
   like the skins; the core keeps only a minimal test plug-in for its
   own tests.
