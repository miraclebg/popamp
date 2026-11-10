# Testing

## Tested platforms

| | Tested on |
|---|---|
| OS | Fedora Linux 44, KDE Plasma Desktop Edition, x86_64 |
| Kernel | 7.0.12-201.fc44 |
| Session | KDE Plasma on Wayland, Popamp running through Xwayland 24.1.12 |
| Audio | PipeWire 1.6.7 via alsa-lib 1.2.16.1 (`default` device) |
| Rust | rustc / cargo 1.96.0 (minimum supported version 1.96 checked with `cargo +1.96 check --locked`) |
| Dialogs | zenity 4.2.2 |

Windows: CI runs clippy and the tests on GitHub's Windows Server 2025
runners, and the release exe was checked by hand under Proton
Experimental (windows, sound, fonts, dialogs, remote commands, media
controls). Not yet checked on a real Windows machine: the bar's strip
shrinking the work area, the media keys, and the timeline, shuffle,
repeat and cover in the media overlay.

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
cargo test --workspace
cargo test --workspace --features popamp/plugins   # adds the plug-in tests
cargo test --workspace --features popamp/plus      # adds the visualizer tests
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
- Window images: every `/RENDER` state of the built-in skin is compared
  pixel by pixel with the PNGs in `apps/popamp/tests/golden/`, on Linux
  and Windows. The states show no playlist text, so system fonts do not
  change them. After a change that is meant to show, rewrite the images
  with `POPAMP_BLESS=1 cargo test -p popamp golden` and look at them
  before committing.
- Command-line parsing and control-socket round trips.
- Tags: each field and the cover written and read back in MP3, FLAC, Ogg
  Vorbis and M4A (silent files in `crates/popamp-audio/tests/fixtures/`,
  made with ffmpeg); tags the editor does not show survive; bad years and
  track numbers are refused; a failed save leaves the file whole. The File
  info and Album art windows as a user drives them.
- Gallery: the catalog, the tabs and the three orders, checksums,
  installing and updating skins and plug-ins, archives that try to
  write outside their folder.
  `cargo test -p popamp gallery_live -- --ignored` installs everything
  popamp.app offers into a folder of its own.

### Published packages

`packaging/check-repos.sh` installs `popamp` and `popamp-plus` from the
published repositories in fresh containers (openSUSE Tumbleweed and Leap
16.0, Fedora 43 and 44, Debian 12 and 13, Ubuntu 22.04 and 24.04), with
the commands of docs/building.md, and checks the version, the installed
files, the libraries, the skin MIME type, a `/RENDER` of the windows,
the skins (and with popamp-plus the plug-ins) from the same repository,
and a clean removal. All eight passed for 0.7.0 on 2026-11-10.

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
  File info, Album art, Preferences) and drag & drop.
- Saving tags of the track that is playing, on Windows.
- Double size.
- A skin with a `region.txt` window shape.
- ReplayGain-tagged files.
- Media keys through the desktop.

Please report problems with these first.

