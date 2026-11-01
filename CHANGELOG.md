# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `/RENDER=<dir>` (with optional `/SKIN=<file>` and files) draws every
  window in a set of fixed states to PNG files without a display or sound
  card, for skin previews and before/after comparisons. The set of states
  follows Ferramp's `--render`.
- Tooltips on the main window: the title bar buttons, the clutter bar and
  the song title explain themselves after the mouse rests on them; elsewhere
  the tooltip shows the program name.
- Skin cursors: the skin's `*.cur` files (including animated ones, first
  frame) replace the mouse pointer over the matching controls, as in the
  original. `usecursors=0` in `linamp.ini` turns them off.

## [0.1.0] - 2026-10-22

First release.

### Added

- **Classic interface:**
  - Main, Equalizer and Playlist Editor windows, drawn from classic skins at
    1x or double size.
  - Docking and snapping, windowshade mode, EasyMove, always-on-top and
    `region.txt` window shapes.
- **Skins:**
  - Classic `.wsz`/`.zip`/folder skins with per-file fallback.
  - `viscolor.txt` and `pledit.txt` support.
  - An original procedurally drawn default skin.
- **Playback:**
  - MP3, AAC/M4A, FLAC, Ogg Vorbis, WAV, AIFF, MKA and ALAC through
    Symphonia, with ALSA output loaded at runtime.
  - Gapless playback, stop with fadeout, stop after current, ReplayGain.
  - HTTP(S)/Icecast/SHOUTcast streams with ICY titles.
  - YouTube videos, playlists and channels through `yt-dlp` and `ffmpeg`.
- **Equalizer:** 10 bands with Winamp or ISO band sets, preamp and
  limiter. `.eqf`/`.q1` preset import/export and per-file auto-load.
- **Playlist:**
  - M3U, M3U8, PLS and XSPF.
  - Sorting, shuffle, repeat, manual advance, drag-reorder, jump to
    file/time.
  - Advanced Title Formatting.
- **Visualization:** classic spectrum analyzer and oscilloscope.
- **Control:**
  - Winamp keyboard shortcuts and menus.
  - Single-instance remote control with Winamp-style `/SWITCH` options and
    command ids.
  - MPRIS2 D-Bus interface.
- **Packaging:** desktop entry, `.wsz` MIME type, icons and a per-user
  install script.

[Unreleased]: https://github.com/sayoojkkarun/linamp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/sayoojkkarun/linamp/releases/tag/v0.1.0
