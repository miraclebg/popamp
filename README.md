<p align="center"><img src="assets/logo/linamp.svg" width="128" alt="Linamp logo"></p>

# Linamp

[![CI](https://github.com/sayoojkkarun/linamp/actions/workflows/ci.yml/badge.svg)](https://github.com/sayoojkkarun/linamp/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A Winamp-style skinnable audio player for Linux, written in Rust. Linamp
contains no Winamp code or artwork. Its default skin is drawn procedurally,
and classic `.wsz` skins load at runtime.

- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [Usage](#usage)
- [YouTube](#youtube)
- [Tested platforms](#tested-platforms)
- [Test procedure](#test-procedure)
- [Known limitations](#known-limitations)
- [Development](#development)
- [Contributing](#contributing)
- [License](#license)

## Features

- **Classic windows:** Main, Equalizer and Playlist Editor, drawn pixel-exact
  from the skin at 1x or double size. Windows dock and snap together, and
  support windowshade mode, EasyMove, always-on-top and `region.txt` shapes.
- **Classic skins:** `.wsz`/`.zip` archives or unpacked directories. Each file
  falls back to the default skin if missing or broken. `viscolor.txt`,
  `pledit.txt` and the skin's mouse cursors (`*.cur`) are supported.
- **Playback:**
  - Formats: MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, MKA (Matroska audio)
    and ALAC, decoded by [Symphonia](https://github.com/pdeljanov/Symphonia).
  - Gapless playback, stop with fadeout, stop after current, ReplayGain.
  - HTTP(S)/Icecast/SHOUTcast streams with ICY titles.
  - YouTube videos, playlists and channels through `yt-dlp` and `ffmpeg`.
- **Equalizer:** 10 bands plus preamp, with either the Winamp or the ISO
  band set and an optional limiter. Reads and writes `.eqf` and `.q1`
  presets, and can auto-load presets per file.
- **Playlist:**
  - M3U, M3U8, PLS and XSPF.
  - Sort, reverse, randomize, crop, remove dead entries.
  - Drag to reorder, jump to file or time.
  - Bookmarks for favourite files and streams (`Alt+I`, main menu).
  - Advanced Title Formatting (`[%artist% - ]$if2(%title%,…)`).
- **Visualization:** classic spectrum analyzer (normal, fire and line styles,
  peaks) and oscilloscope (dots, lines, solid).
- **Control:**
  - Winamp keyboard shortcuts and Winamp-style menus.
  - Command line with `/PLAY /NEXT /VOL=50 /COMMAND=40045 …` sent to the
    running instance.
  - MPRIS2 for media keys, desktop widgets and `playerctl`.

## Requirements

### Build

| Need | Version | Why |
|---|---|---|
| Rust toolchain (`rustc`, `cargo`) | 1.89 or newer | compiler (install with [rustup](https://rustup.rs)) |
| C compiler (`gcc` or `clang`) | any | the `ring` crate (TLS for HTTPS streams) compiles C code |
| Internet access | — | `cargo` downloads crates on the first build |

No `-devel`/`-dev` packages and no `pkg-config` are needed. The system
libraries below are loaded at runtime, not linked at build time.

### Runtime

| Library / tool | Required? | Used for |
|---|---|---|
| X11 server or XWayland | **required** | windows (Linamp always uses the X11 backend, including on Wayland desktops) |
| `libasound.so.2` (ALSA) | **required** | audio output; on PipeWire/PulseAudio it goes through their ALSA `default` device |
| `libX11`, `libX11-xcb`, `libxcb`, `libXcursor`, `libXi`, `libxkbcommon`, `libxkbcommon-x11` | **required** | windowing and keyboard (loaded with `dlopen`) |
| `fontconfig` (`fc-match`) and any TrueType font | recommended | playlist and menu text (blank without them) |
| `zenity` or `kdialog` | recommended | open-file, open-URL, save, preset and jump-to dialogs |
| D-Bus session bus | optional | MPRIS (media keys, desktop widgets) |
| `yt-dlp` | optional | YouTube links |
| `ffmpeg` | optional | YouTube links |

### Packages per distribution

Fedora was used for testing. The Debian/Ubuntu and Arch names are the usual
package names but **have not been tested**.

**Fedora**

```sh
sudo dnf install gcc alsa-lib libX11 libX11-xcb libxcb libXcursor libXi \
    libxkbcommon libxkbcommon-x11 fontconfig zenity
sudo dnf install yt-dlp ffmpeg-free          # optional, for YouTube
```

**Debian / Ubuntu** (untested)

```sh
sudo apt install build-essential libasound2t64 libx11-6 libx11-xcb1 libxcb1 \
    libxcursor1 libxi6 libxkbcommon0 libxkbcommon-x11-0 fontconfig zenity
sudo apt install yt-dlp ffmpeg               # optional, for YouTube
```

On Debian 12 and Ubuntu before 24.04 the ALSA package is `libasound2`. The
distribution's `yt-dlp` is often too old for YouTube; prefer
`pipx install yt-dlp`.

**Arch Linux** (untested)

```sh
sudo pacman -S --needed base-devel alsa-lib libx11 libxcb libxcursor libxi \
    libxkbcommon libxkbcommon-x11 fontconfig zenity
sudo pacman -S yt-dlp ffmpeg                 # optional, for YouTube
```

## Installation

### 1. Get Rust (if you don't have it)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Build and run from the source tree

```sh
git clone https://github.com/sayoojkkarun/linamp.git && cd linamp
cargo build --release
./target/release/linamp ~/Music
```

The first build downloads and compiles all crates (a few minutes). The
binary is `target/release/linamp`.

### 3. Install for the current user (optional)

```sh
packaging/install.sh
```

This builds a release binary and installs:

| File | Destination |
|---|---|
| `linamp` binary | `~/.local/bin/linamp` |
| Desktop entry | `~/.local/share/applications/linamp.desktop` |
| `.wsz` MIME type | `~/.local/share/mime/packages/linamp-skin.xml` |
| Icons (SVG + 16–256 px PNG) | `~/.local/share/icons/hicolor/*/apps/linamp.*` |
| Skins folder | `~/.local/share/linamp/skins/` (created empty) |

Make sure `~/.local/bin` is on your `PATH`. For a system-wide install, run
`sudo PREFIX=/usr/local packaging/install.sh`.

### Uninstall

```sh
rm -f ~/.local/bin/linamp ~/.local/share/applications/linamp.desktop \
      ~/.local/share/mime/packages/linamp-skin.xml
rm -f ~/.local/share/icons/hicolor/*/apps/linamp.png \
      ~/.local/share/icons/hicolor/scalable/apps/linamp.svg
update-mime-database ~/.local/share/mime
# Settings and skins, if you want them gone too:
rm -rf ~/.config/linamp ~/.local/share/linamp
```

## Usage

| Action | How |
|---|---|
| Open files | `L`, the eject button, or drag files onto a window |
| Open a URL / stream / YouTube link | `Ctrl+L`, or `linamp <url>` |
| Transport | `Z X C V B` (prev, play, pause, stop, next) |
| Seek / volume | arrow keys, or drag the bars; mouse wheel changes volume |
| Main menu | click the top-left button, right-click, or `Alt+F` |
| Skins | `Alt+S`, or put `.wsz` files in `~/.local/share/linamp/skins/` |
| Equalizer keys | `1`–`0` / `Q`–`P` raise/lower bands, `N` on/off, `S` presets |
| Double size / shade | `Ctrl+D` / `Ctrl+W` (or double-click the title bar) |

Remote control (talks to the running instance):

```sh
linamp /ADD song.flac      # enqueue
linamp /PLAYPAUSE
linamp /VOL=40 /SHUFFLE=1
linamp /STATUS             # JSON status
linamp /NEW                # start a second, independent instance
linamp /HELP               # all options
```

Settings live in `~/.config/linamp/linamp.ini`. Its key names follow
`winamp.ini`. The playlist is saved to `~/.config/linamp/linamp.m3u8`.

## YouTube

Install `yt-dlp` and `ffmpeg`, then open a YouTube link like any other URL:

- **A video** (`watch?v=…`, `youtu.be/…`, `/shorts/…`) plays as one track.
- **A playlist or channel** (`playlist?list=…`, `/@name/videos`) expands into
  its videos, up to 500.
- **Saved playlists** keep the YouTube page URL. The audio URL expires, so it
  is looked up again (cached for an hour) whenever the track plays.
- **Seeking** restarts `ffmpeg` at the new position. Live streams cannot be
  seeked.
- **Startup** takes 2–3 seconds while `yt-dlp` resolves the video.

Settings in `linamp.ini`: `ytdlp_path`, `ffmpeg_path` and `ytdlp_format`
(default `bestaudio[ext=m4a]/bestaudio`). The HTTP `proxy` setting is passed
to both tools. Keep `yt-dlp` up to date, because YouTube changes often break
old versions. Recent `yt-dlp` versions also want a JavaScript runtime such
as `deno` for full format support.

## Tested platforms

| | Tested on |
|---|---|
| OS | Fedora Linux 44, KDE Plasma Desktop Edition, x86_64 |
| Kernel | 7.0.12-201.fc44 |
| Session | KDE Plasma on Wayland, Linamp running through Xwayland 24.1.12 |
| Audio | PipeWire 1.6.7 via alsa-lib 1.2.16.1 (`default` device) |
| Rust | rustc / cargo 1.96.0 (minimum supported version 1.89 checked with `cargo +1.89 check --locked`) |
| Dialogs | zenity 4.2.2 |
| YouTube tools | yt-dlp 2026.10.21, ffmpeg-free 8.1.2 |

**Not tested:**
- Other distributions, GNOME and other desktops.
- Plain X11 (non-Wayland) sessions.
- PulseAudio-only or pure-ALSA systems.
- Non-x86_64 CPUs.
- HiDPI scaling.

## Test procedure

### Automated

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets    # expected: no warnings
cargo test --workspace                    # 57 unit tests
```

The unit tests cover:
- Config parsing and round trips, and title formatting (ATF).
- Playlist navigation, shuffle (each track once per cycle) and editing.
- Playlist files: M3U/PLS/XSPF round trips, relative paths.
- EQF/Q1 presets: byte-exact round trip and dB scale.
- EQ: frequency response of each band ±1.5 dB, limiter ceiling.
- Skins: sprite rectangles inside their sheets, skin fallbacks, region
  rasterization, glyph mapping.
- ICY metadata parsing, YouTube URL detection and `yt-dlp` JSON parsing.
- Command-line parsing and control-socket round trips.

### Manual checks performed

All playback checks ran at volume 0 through the real ALSA/PipeWire output.

| Check | How | Result |
|---|---|---|
| Local formats | `cargo run --release -p linamp-audio --example play -- --vol 0 <file>` with one file per format (made from the same track with `ffmpeg`) | MP3, FLAC, Ogg Vorbis, WAV, AIFF, M4A (AAC), MKA (FLAC) play with correct length and bitrate. **Opus fails** (unsupported). |
| Gapless and end of list | `play` example with two files | next track starts without a gap; "finished" is reported after the last one |
| Internet radio | `play` example with `https://ice1.somafm.com/groovesalad-128-mp3` | plays at 127 kbps; station name and live ICY song titles shown |
| YouTube | `linamp <watch-url>`, then `/STATUS`, two quick `/COMMAND=40148` (+5 s), and `/ADD <channel-url>` | plays with title, uploader and length. Seeking lands at the right time. The channel expands into the playlist. |
| Window rendering | `LINAMP_SNAPSHOT=<dir> linamp /NEW /INIDIR=<tmp>` renders the windows to PNG | Default skin and the original Winamp 5.9.2 base skin both line up exactly. Windowshade, wide playlist (with vis) and oscilloscope modes correct. |
| Skins | `linamp Base.wsz` | `.wsz` is installed to the skins folder and applied; loading the archive matches loading the unpacked folder |
| Remote control | `/PLAY /PAUSE /PLAYPAUSE /NEXT /STOP /VOL= /SHUFFLE= /COMMAND= /STATUS /QUIT` against a running instance | all work |
| MPRIS | `gdbus call --session -d org.mpris.MediaPlayer2.linamp …` (Play, Next, Properties.GetAll, Quit) | all work; metadata and position correct |
| Window icon | `xprop -name Linamp _NET_WM_ICON` | 64×64 icon set |
| Session persistence | quit, inspect `linamp.ini` / `linamp.m3u8` | settings, positions and playlist saved |

**Not yet tested by hand:**
- Mouse interaction: dragging, docking and snapping, sliders, pop-up menus,
  playlist drag-reorder.
- Keyboard shortcuts.
- The `zenity`/`kdialog` dialogs and drag & drop.
- Double size.
- A skin with a `region.txt` window shape.
- ReplayGain-tagged files.
- Media keys through the desktop.

Please report problems with these first.

## Known limitations

- Opus, MIDI and tracker modules are not supported (YouTube Opus streams
  still play, because `ffmpeg` decodes them).
- No native Wayland windows: Linamp needs Xwayland on Wayland desktops.
- Starting a YouTube track blocks other commands (such as Stop) for the 2–3 s
  that `yt-dlp` needs.
- No preferences window; options are in the menus and `linamp.ini`.
- No tag editing, album art, tray icon, media library or Modern (`.wal`)
  skins yet. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run -p linamp-audio --example play -- --vol 0 file.mp3   # headless engine test
cargo run -p linamp-skin --example dump_sheets -- skin.wsz out/ # dump sprite sheets
LINAMP_SNAPSHOT=out/ linamp /NEW /INIDIR=/tmp/cfg              # render windows to PNG and exit
linamp /RENDER=out/ /SKIN=skin.wsz [files]                     # same without a display: fixed states
```

`/RENDER` needs no X server or sound card. It writes one PNG per window and
state (stopped, playing with spectrum or oscilloscope, toggles, equalizer
values and a preset, windowshade, small and wide playlist) using the
settings in `out/linamp.ini` if that file exists.

| Crate | Contents |
|---|---|
| `crates/linamp-core` | settings, playlist model and formats, title formatting, EQF, command ids |
| `crates/linamp-audio` | decoding, EQ, limiter, ALSA output, engine thread, streams, YouTube |
| `crates/linamp-skin` | skin loading, sprite geometry, default skin generator |
| `apps/linamp` | windows, menus, input, dialogs, IPC, MPRIS |

## Contributing

Bug reports and pull requests are welcome; see
[CONTRIBUTING.md](CONTRIBUTING.md). The "Not yet tested by hand" list above
is a good place to start. Changes are recorded in
[CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

Third-party crates keep their own licenses. Notably, the Symphonia decoders
are MPL-2.0. Code adapted from other projects is listed in
[THIRD_PARTY.md](THIRD_PARTY.md). Winamp is a trademark of its owner; Linamp is an independent
project and is not affiliated with or endorsed by it.
