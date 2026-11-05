# Plug-ins: design

Status: **draft for agreement**. Nothing here is built yet.

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

## Runtime: WebAssembly components

A plug-in is a WebAssembly component (`plugin.wasm`) run by **wasmtime**,
with its interface written in WIT. The core is the host.

wasmtime against extism:

- wasmtime's component model gives typed calls generated from the WIT
  file on both sides. A version mismatch fails at load time, with a
  message, rather than as bad data at run time.
- extism passes bytes and leaves the encoding to each plug-in (usually
  JSON). That is simpler to start with, but every DSP block would be
  serialised, and the interface would live in documentation, not in a
  checked file.
- wasmtime has fuel and epoch interruption, which the time limits below
  need.

**Proposal: wasmtime.** The cost is a larger binary (several MB for
Cranelift) and a longer build. Plug-ins are compiled to native code once
and cached in `~/.cache/popamp/plugins`.

## Where plug-ins live

```
~/.local/share/popamp/plugins/<id>/
    plugin.toml      manifest
    plugin.wasm      the component
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
api = "1.0"                    # the WIT package version it was built for
kind = "source"
description = "Plays YouTube videos and playlists through yt-dlp."

[permissions]
exec = ["yt-dlp", "ffmpeg"]    # programs it may start (see Security)
net = false                    # HTTP through the host

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
- **`net`**: the plug-in may make HTTP(S) requests through the host,
  which applies the user's proxy.

Rejected alternative: plug-ins as separate processes speaking JSON over
standard input and output. They wrap programs such as yt-dlp naturally,
but they are native code with all the user's rights and no sandbox at
all, and SPEC rules native plug-ins out.

## How a source plug-in plays

The plug-in never handles audio. It answers questions, and the core does
the playing:

```wit
package popamp:plugin@1.0.0;

interface source {
    record track { location: string, title: option<string>, length-ms: option<u64> }
    /// Tags as in the core's TrackMeta: title, artist, album, length...
    record meta { title: option<string>, artist: option<string>, length-ms: option<u64>, is-live: bool }

    variant how-to-play {
        /// The core opens this URL like any HTTP stream.
        http(tuple<string, list<tuple<string, string>>>),     // url, headers
        /// The core starts this program (one of `exec`) and decodes its
        /// standard output, raw samples in the given format.
        pipe(pipe-spec),
    }
    record pipe-spec { program: string, args: list<string>, rate: u32, channels: u8, seekable: bool }

    /// Whether this plug-in takes the link. Must be quick: no exec, no net.
    handles: func(location: string) -> bool;
    /// A link to a list (a playlist, a channel) as tracks.
    expand: func(location: string) -> result<list<track>, string>;
    /// Title, length and other tags for one track.
    describe: func(location: string) -> result<meta, string>;
    /// How to play one track, starting at `start-ms`.
    open: func(location: string, start-ms: u64) -> result<how-to-play, string>;
}
```

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
- `handles` must return within 10 ms (wasmtime epoch interruption);
  `expand`, `describe` and `open` within 60 s. A plug-in that traps or
  runs out of time fails that call; three failures in a row switch it
  off until the user turns it on again.
- A `dsp` plug-in runs on the decoding thread, never in the callback,
  over blocks of 1024 frames in shared memory. The output buffer
  (`buffer_ms`) already sits between it and the sound card. A block over
  its time budget passes through unchanged; repeated overruns switch the
  plug-in off.

## Versions

The WIT package is `popamp:plugin@MAJOR.MINOR`. Popamp loads plug-ins
built for its major version and the same or an older minor version;
minor versions only add. A plug-in built for another major version stays
off, with a message naming both versions.

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
3. Host: wasmtime, manifest, permissions dialog, the "Plug-ins…" window,
   settings form.
4. The YouTube plug-in in its own repository, `popamp/plugin-youtube`;
   YouTube code leaves the core.
5. The `dsp` interface and a small example plug-in.
6. Later: `decoder` (version 2), `vis`.

## Open questions

1. wasmtime (proposed) or extism?
2. `exec` as described, limited to programs named in the manifest and
   confirmed by the user: acceptable?
3. Version 1 with `source` and `dsp` only, `decoder` in version 2?
4. Example plug-ins: SPEC says `examples/plugins/` in this repository.
   Proposal: each one in a repository of its own, like the skins, and
   only a minimal test plug-in inside the core for its tests.
