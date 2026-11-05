# Plug-ins: design

Status: **agreed 2026-11-05**. Nothing here is built yet.

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

- Calls pass JSON. The types live in one small crate, `popamp-plugin-api`,
  used by the core and by plug-ins written in Rust, so both sides are
  checked by the compiler. This document lists the calls; the crate is
  their definition.
- Audio blocks for `dsp` plug-ins pass as raw little-endian `f32`
  samples, not JSON.

Plug-ins are compiled to native code by wasmtime when loaded; the binary
grows by several MB for that compiler.

## Where plug-ins live

```
~/.local/share/popamp/plugins/<id>/
    plugin.toml      manifest
    plugin.wasm      the module
```

A plug-in can be installed by cloning its repository there, like a skin
(hidden files such as `.git` are ignored). The core ships no plug-ins.
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

[permissions]
exec = ["yt-dlp", "ffmpeg"]    # programs it may start (see Security)
net = []                       # hosts it may reach over HTTP

[[setting]]
key = "ytdlp"
label = "yt-dlp program"
type = "path"
default = "yt-dlp"

[[setting]]
key = "format"
label = "Format"
type = "text"
default = "bestaudio[ext=m4a]/bestaudio"
```

The core checks the manifest strictly, like `skin.toml`: an unknown key
or a missing file keeps the plug-in off, and the error is shown.
Settings are stored in `popamp.ini` under `[plugin.<id>]`; the core draws
their form, so plug-ins draw nothing.

## Security

By default a plug-in can compute and nothing else: no files, no network,
no clock beyond what the host passes in. Two permissions widen this, and
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
  (`net = ["*.example.com"]`); the core applies the user's proxy.

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
| `handles` | a location | `true` if this plug-in takes it; quick: no exec, no net |
| `expand` | a location of a list (a playlist, a channel) | its tracks: location, title, length |
| `describe` | a location | its tags: title, artist, length, whether it is live |
| `open` | a location and a start time | how to play it, below |

`open` answers one of:

- `http`: a URL and headers; the core opens it like any HTTP stream;
- `pipe`: a program (one of `exec`), its arguments, and the sample rate,
  channels and seekability of the raw samples it writes to standard
  output; the core starts it and plays that output.

An error is a string the core shows in the title display.

For YouTube, `open` runs yt-dlp, reads the stream URL and headers from
its JSON and returns a `pipe` that starts ffmpeg on that URL: what
`PipeSource` does today, split so that the core runs ffmpeg.

## Threads and time limits

- **No plug-in call runs on the audio callback, the UI thread or the
  engine thread.** `expand`, `describe` and `open` run on a pool of
  worker threads. The engine asks for a track, goes on handling
  commands, and receives the opened source when it is ready. Stop or a
  new track while a call is running drops its result when it arrives.
- This also fixes the core's own HTTP streams, which today connect on
  the engine thread, so Stop does not work while a radio station is
  connecting. It is the first step of the work below and is useful
  without any plug-in.
- `handles` must return within 10 ms (Extism's call timeout);
  `expand`, `describe` and `open` within 60 s. A plug-in that traps or
  runs out of time fails that call; three failures in a row switch it
  off until the user turns it on again.
- A `dsp` plug-in runs on the decoding thread, never in the callback,
  over blocks of 1024 frames in shared memory. The output buffer
  (`buffer_ms`) already sits between it and the sound card. A block over
  its time budget passes through unchanged; repeated overruns switch the
  plug-in off.

## Versions

The API version is `MAJOR.MINOR`, returned by the plug-in's `api_version`
call and written in its manifest. Popamp loads plug-ins built for its
major version and the same or an older minor version; minor versions
only add calls or optional fields. A plug-in built for another major
version stays off, with a message naming both versions.

## What leaves the core

When the YouTube plug-in works:

- `crates/popamp-audio/src/youtube.rs` is deleted;
- the settings `ytdlp_path`, `ffmpeg_path` and `ytdlp_format` move to
  `[plugin.youtube]`;
- `model.rs` asks the source plug-ins instead of calling `youtube::*`;
  "CONNECTING TO YOUTUBE…" becomes "CONNECTING…" for any slow source;
- a YouTube link with the plug-in missing or off fails like any
  unplayable entry: "no plug-in plays this link".

## Order of work

1. Core: open tracks off the engine thread, for files and HTTP streams.
   Stop works while connecting.
2. Core: the source interface inside the player, with the built-in HTTP
   and file sources behind it (SPEC section 6).
3. Host: Extism, the `popamp-plugin-api` crate, manifest, permissions dialog, the "Plug-ins…" window,
   settings form.
4. The YouTube plug-in in its own repository, `popamp/plugin-youtube`;
   YouTube code leaves the core.
5. The `dsp` interface and a small example plug-in.
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
