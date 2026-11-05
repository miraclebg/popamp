<p align="center"><img src="assets/logo/popamp.svg" width="128" alt="Popamp logo"></p>

# Popamp

[![CI](https://github.com/popamp/popamp/actions/workflows/ci.yml/badge.svg)](https://github.com/popamp/popamp/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A Winamp-style skinnable audio player for Linux, written in Rust. Popamp
contains no Winamp code or artwork. Its default skin is its own vector skin
(`skins/default/`), and classic `.wsz` skins load at runtime.

Popamp grew out of [Linamp](https://github.com/sayoojkkarun/linamp) by
Sayooj K Karun and keeps its history; see [THIRD_PARTY.md](THIRD_PARTY.md).

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

- **Classic windows:** Main, Equalizer and Playlist Editor at any size from
  x1 to x3 in steps of 0.25 (`Ctrl+D` switches between x1 and the chosen
  size). Classic skins stay crisp at fractional sizes. Windows dock and snap together, and
  support windowshade mode, EasyMove, always-on-top and `region.txt` shapes.
  Tooltips explain the main window's buttons.
- **Own dialogs:** Jump to file (search as you type), File info and a
  Preferences window (`Ctrl+P`). Opening and saving files uses the desktop's
  dialog through `zenity` or `kdialog`.
- **Vector skins** (`.psz`): drawn in SVG, sharp at every size, with TrueType text
  and live reload while you edit them. Popamp's default skin is one. See
  [docs/skin-format.md](docs/skin-format.md).
- **Classic skins:** `.wsz`/`.zip` archives or unpacked directories. Each file
  falls back to the default skin if missing or broken. `viscolor.txt`,
  `pledit.txt` and the skin's mouse cursors (`*.cur`) are supported.
  Skins > Skin Museum browses, searches and downloads skins from
  [skins.webamp.org](https://skins.webamp.org); Popamp contacts the
  museum only while that window is open.
- **Playback:**
  - Formats: MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, MKA (Matroska audio)
    and ALAC, decoded by [Symphonia](https://github.com/pdeljanov/Symphonia).
  - Gapless playback, stop with fadeout, stop after current, ReplayGain.
  - HTTP(S)/Icecast/SHOUTcast streams with ICY titles.
  - YouTube videos, playlists and channels through the
    [YouTube plug-in](https://github.com/popamp/plugin-youtube).
- **Equalizer:** 10 bands plus preamp, with either the Winamp or the ISO
  band set and an optional limiter. Reads and writes `.eqf` and `.q1`
  presets, and can auto-load presets per file.
- **Playlist:**
  - M3U, M3U8, PLS and XSPF.
  - Sort, reverse, randomize, crop, remove dead entries.
  - Drag to reorder, jump to time.
  - Bookmarks for favourite files and streams (`Alt+I`, main menu).
  - Export as an HTML page (MISC > Misc options).
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
| X11 server or XWayland | **required** | windows (Popamp always uses the X11 backend, including on Wayland desktops) |
| `libasound.so.2` (ALSA) | **required** | audio output; on PipeWire/PulseAudio it goes through their ALSA `default` device |
| `libX11`, `libX11-xcb`, `libxcb`, `libXcursor`, `libXi`, `libxkbcommon`, `libxkbcommon-x11` | **required** | windowing and keyboard (loaded with `dlopen`) |
| `fontconfig` (`fc-match`) and any TrueType font | recommended | playlist and menu text (blank without them) |
| `zenity` or `kdialog` | recommended | open-file, open-URL, save and preset dialogs |
| D-Bus session bus | optional | MPRIS (media keys, desktop widgets) |

### Packages per distribution

Fedora was used for testing. The Debian/Ubuntu and Arch names are the usual
package names but **have not been tested**.

**Fedora**

```sh
sudo dnf install gcc alsa-lib libX11 libX11-xcb libxcb libXcursor libXi \
    libxkbcommon libxkbcommon-x11 fontconfig zenity
```

**Debian / Ubuntu** (untested)

```sh
sudo apt install build-essential libasound2t64 libx11-6 libx11-xcb1 libxcb1 \
    libxcursor1 libxi6 libxkbcommon0 libxkbcommon-x11-0 fontconfig zenity
```

On Debian 12 and Ubuntu before 24.04 the ALSA package is `libasound2`.

**Arch Linux** (untested)

```sh
sudo pacman -S --needed base-devel alsa-lib libx11 libxcb libxcursor libxi \
    libxkbcommon libxkbcommon-x11 fontconfig zenity
```

## Installation

### Packages

Releases on the [Releases page](https://github.com/popamp/popamp/releases)
come with packages for x86_64 and ARM64 (built on Ubuntu 22.04, so they
need glibc 2.35 or newer):

| System | Package |
|---|---|
| Debian / Ubuntu | `sudo apt install ./popamp-<version>-linux-amd64.deb` (ARM64: `-arm64.deb`) |
| Anything else | `Popamp-<version>-linux-x86_64.AppImage`: `chmod +x` it and run it |

The AppImage uses the system's ALSA, X11 and xkbcommon libraries, which
nearly every desktop has. To build the packages yourself after
`cargo build --release`:

```sh
packaging/build-deb.sh        # needs dpkg-deb and binutils
packaging/build-appimage.sh   # needs appimagetool
```

Both write to `dist/`. Or build from source:

### 1. Get Rust (if you don't have it)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

### 2. Build and run from the source tree

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release
./target/release/popamp ~/Music
```

The first build downloads and compiles all crates (a few minutes). The
binary is `target/release/popamp`.

This builds the player and its skins. For source plug-ins (see
[docs/plugins.md](docs/plugins.md)), build with the `plugins` feature;
it adds a WebAssembly runtime, so the build takes longer and the binary
is larger:

```sh
cargo build --release --features plugins
```

### 3. Install for the current user (optional)

```sh
packaging/install.sh
```

This builds a release binary and installs:

| File | Destination |
|---|---|
| `popamp` binary | `~/.local/bin/popamp` |
| Desktop entry | `~/.local/share/applications/popamp.desktop` |
| `.wsz`/`.psz` MIME types | `~/.local/share/mime/packages/popamp-skin.xml` |
| Icons (SVG + 16–256 px PNG) | `~/.local/share/icons/hicolor/*/apps/popamp.*` |
| Skins folder | `~/.local/share/popamp/skins/` (created empty) |

Make sure `~/.local/bin` is on your `PATH`. For a system-wide install, run
`sudo PREFIX=/usr/local packaging/install.sh`.

### Uninstall

```sh
rm -f ~/.local/bin/popamp ~/.local/share/applications/popamp.desktop \
      ~/.local/share/mime/packages/popamp-skin.xml \
      ~/.local/share/metainfo/popamp.metainfo.xml
rm -f ~/.local/share/icons/hicolor/*/apps/popamp.png \
      ~/.local/share/icons/hicolor/scalable/apps/popamp.svg
update-mime-database ~/.local/share/mime
# Settings and skins, if you want them gone too:
rm -rf ~/.config/popamp ~/.local/share/popamp
```

## Usage

| Action | How |
|---|---|
| Open files | `L`, the eject button, or drag files onto a window |
| Open a URL / stream / YouTube link | `Ctrl+L`, or `popamp <url>` |
| Transport | `Z X C V B` (prev, play, pause, stop, next) |
| Seek / volume | arrow keys, or drag the bars; mouse wheel changes volume |
| Main menu | click the top-left button, right-click, or `Alt+F` |
| Skins | `Alt+S`, or put `.wsz`/`.psz` files in `~/.local/share/popamp/skins/` |
| Equalizer keys | `1`–`0` / `Q`–`P` raise/lower bands, `N` on/off, `S` presets |
| Double size / shade | `Ctrl+D` / `Ctrl+W` (or double-click the title bar) |

Remote control (talks to the running instance):

```sh
popamp /ADD song.flac      # enqueue
popamp /PLAYPAUSE
popamp /VOL=40 /SHUFFLE=1
popamp /STATUS             # JSON status
popamp /NEW                # start a second, independent instance
popamp /HELP               # all options
```

Settings live in `~/.config/popamp/popamp.ini`. Its key names follow
`winamp.ini`. The playlist is saved to `~/.config/popamp/popamp.m3u8`.

## YouTube

YouTube is a plug-in, [plugin-youtube](https://github.com/popamp/plugin-youtube),
so it needs a build with plug-ins (`--features plugins`) and the plug-in
installed; its README says how. It plays videos, playlists and channels
through `yt-dlp` and `ffmpeg`. Without it a YouTube link fails with "no
plug-in plays this link".

The settings `ytdlp_path`, `ffmpeg_path` and `ytdlp_format` of earlier
versions move to the plug-in's settings (`[plugin.youtube]` in
`popamp.ini`) the first time the new version starts.

## Tested platforms

| | Tested on |
|---|---|
| OS | Fedora Linux 44, KDE Plasma Desktop Edition, x86_64 |
| Kernel | 7.0.12-201.fc44 |
| Session | KDE Plasma on Wayland, Popamp running through Xwayland 24.1.12 |
| Audio | PipeWire 1.6.7 via alsa-lib 1.2.16.1 (`default` device) |
| Rust | rustc / cargo 1.96.0 (minimum supported version 1.89 checked with `cargo +1.89 check --locked`) |
| Dialogs | zenity 4.2.2 |

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
- ICY metadata parsing; a web page is not taken for a stream.
- Plug-ins: manifests, permissions and calls through a test plug-in.
- Command-line parsing and control-socket round trips.

### Manual checks performed

All playback checks ran at volume 0 through the real ALSA/PipeWire output.

| Check | How | Result |
|---|---|---|
| Local formats | `cargo run --release -p popamp-audio --example play -- --vol 0 <file>` with one file per format (made from the same track with `ffmpeg`) | MP3, FLAC, Ogg Vorbis, WAV, AIFF, M4A (AAC), MKA (FLAC) play with correct length and bitrate. **Opus fails** (unsupported). |
| Gapless and end of list | `play` example with two files | next track starts without a gap; "finished" is reported after the last one |
| Internet radio | `play` example with `https://ice1.somafm.com/groovesalad-128-mp3` | plays at 127 kbps; station name and live ICY song titles shown |
| Window rendering | `POPAMP_SNAPSHOT=<dir> popamp /NEW /INIDIR=<tmp>` renders the windows to PNG | Default skin and the original Winamp 5.9.2 base skin both line up exactly. Windowshade, wide playlist (with vis) and oscilloscope modes correct. |
| Skins | `popamp Base.wsz` | `.wsz` is installed to the skins folder and applied; loading the archive matches loading the unpacked folder |
| Remote control | `/PLAY /PAUSE /PLAYPAUSE /NEXT /STOP /VOL= /SHUFFLE= /COMMAND= /STATUS /QUIT` against a running instance | all work |
| MPRIS | `gdbus call --session -d org.mpris.MediaPlayer2.popamp …` (Play, Next, Properties.GetAll, Quit) | all work; metadata and position correct |
| Window icon | `xprop -name Popamp _NET_WM_ICON` | 64×64 icon set |
| Session persistence | quit, inspect `popamp.ini` / `popamp.m3u8` | settings, positions and playlist saved |

**Not yet tested by hand:**
- Mouse interaction: dragging, docking and snapping, sliders, pop-up menus,
  playlist drag-reorder.
- Keyboard shortcuts.
- The `zenity`/`kdialog` dialogs, Popamp's own dialogs (Jump to file,
  File info, Preferences) and drag & drop.
- Double size.
- A skin with a `region.txt` window shape.
- ReplayGain-tagged files.
- Media keys through the desktop.

Please report problems with these first.

## Known limitations

- Opus, MIDI and tracker modules are not supported.
- No native Wayland windows: Popamp needs Xwayland on Wayland desktops.
- The Preferences window (`Ctrl+P`) covers the common options; the rest are
  in the menus and `popamp.ini`.
- No tag editing, album art, tray icon, media library or Modern (`.wal`)
  skins yet. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run -p popamp-audio --example play -- --vol 0 file.mp3   # headless engine test
cargo run -p popamp-skin --example dump_sheets -- skin.wsz out/ # dump sprite sheets
POPAMP_SNAPSHOT=out/ popamp /NEW /INIDIR=/tmp/cfg              # render windows to PNG and exit
popamp /RENDER=out/ /SKIN=skin.wsz [files]                     # same without a display: fixed states
```

`/RENDER` needs no X server or sound card. It writes one PNG per window and
state (stopped, playing with spectrum or oscilloscope, toggles, equalizer
values and a preset, windowshade, small and wide playlist) using the
settings in `out/popamp.ini` if that file exists.

| Crate | Contents |
|---|---|
| `crates/popamp-core` | settings, playlist model and formats, title formatting, EQF, command ids |
| `crates/popamp-audio` | decoding, EQ, limiter, ALSA output, engine thread, sources, streams |
| `crates/popamp-skin` | skin loading, sprite geometry, default skin generator |
| `apps/popamp` | windows, menus, input, dialogs, IPC, MPRIS |

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
[THIRD_PARTY.md](THIRD_PARTY.md). Winamp is a trademark of its owner; Popamp is an independent
project and is not affiliated with or endorsed by it.
