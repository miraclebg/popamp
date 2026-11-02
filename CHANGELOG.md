# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- The project is now Popamp, a separate project grown from
  [Linamp](https://github.com/sayoojkkarun/linamp) 0.1.0. The program,
  crates, packages and settings folders (`~/.config/popamp`,
  `~/.local/share/popamp`) carry the new name.

### Added

- `.deb` and AppImage packages for x86_64 and ARM64, built by a release
  workflow on version tags (`packaging/build-deb.sh`,
  `packaging/build-appimage.sh`), and AppStream metadata.
- `/RENDER=<dir>` (with optional `/SKIN=<file>` and files) draws every
  window in a set of fixed states to PNG files without a display or sound
  card, for skin previews and before/after comparisons. The set of states
  follows Ferramp's `--render`.
- Tooltips on the main window: the title bar buttons, the clutter bar and
  the song title explain themselves after the mouse rests on them; elsewhere
  the tooltip shows the program name.
- Skin cursors: the skin's `*.cur` files (including animated ones, first
  frame) replace the mouse pointer over the matching controls, as in the
  original. `usecursors=0` in `popamp.ini` turns them off.
- Bookmarks: `Alt+I` bookmarks the playing track (in the playlist: the
  selected ones), the main menu's Bookmarks submenu plays or removes them
  (`Ctrl+Alt+I`). They are kept in `bookmarks.m3u8` next to `popamp.ini`.
  Remote command ids 50021–50023.
- Equalizer Presets > Load/Save > Default: keep your own default curve and
  return to it in one click (ids 50024/50025).
- Playlist MISC > Misc options > Generate HTML playlist… saves the playlist
  as a web page with titles, lengths and the total (id 50026). The general
  options stay reachable there as a submenu.
- Popamp's own dialog windows, drawn like its menus and no longer needing
  `zenity`:
  - Jump to file (`J`) filters the playlist as you type; the arrows pick,
    Enter plays.
  - File info (`Alt+3`) lists tags and format of the selected track.
  - Preferences (`Ctrl+P`, previously the options menu): window behaviour,
    playlist display and auto-advance take effect at once; playlist font
    size, title format, fade-out, output buffer and HTTP proxy apply on OK.
- Skin Museum (main menu > Skins, id 50027): browse or search the Winamp
  Skin Museum at skins.webamp.org with previews, then download a skin into
  the skins folder and use it. Downloads are checked to be skins before
  they are saved; the HTTP proxy setting applies.

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

[Unreleased]: https://github.com/popamp/popamp/commits/main
[0.1.0]: https://github.com/sayoojkkarun/linamp/releases/tag/v0.1.0
