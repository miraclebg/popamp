# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- The default skin has a repository of its own,
  [popamp-skin-default](https://github.com/popamp/popamp-skin-default), and
  is still built into the program. It can be forked without the player.

### Fixed

- Bringing the player up from its taskbar icon, the desktop's media
  controls or by starting it again raises the equalizer and the playlist
  too, not only the main window. On X11 the two are tied to the main
  window, so they also minimize with it.
- The playlist no longer grows past the bottom or the right of the screen
  when resized, which made the window manager push it over the equalizer.
- A bigger window size shortens a playlist that would no longer fit, so
  its resize grip stays on the screen.

## [0.7.0] - 2026-11-09

### Added

- **Visualizer plug-ins** (`--features visualizers`, part of popamp-plus):
  a WebAssembly plug-in supplies WGSL shaders and the player makes every
  GPU call, in a window of its own that can go full screen. While it is
  full screen and music is playing, the screen saver and monitor sleep
  stay off. The first one is
  [Spectrum](https://github.com/popamp/plugin-spectrum).
- A journal for each plug-in records every command, network request and
  file read, allowed or refused.

### Changed

- **A plug-in now runs only the commands its manifest spells out.**
  `permissions.exec` is a list of command templates: you see each full
  command when you allow it, any change asks again, and the plug-in only
  fills in typed values. A plug-in made for 0.6.0 that runs programs has
  to be updated; [YouTube](https://github.com/popamp/plugin-youtube)
  1.1.0 is.
- **You decide what a plug-in may reach and read.** Any address can be
  unticked. An unusual port, this computer or your network must be
  written out in the manifest and stay off until ticked; redirects are
  not followed. A plug-in that reads files gets the folders you choose.
- The taskbar icon at 32 px is the bolt, like the larger sizes.
- The default skin is drawn from whole shapes instead of thousands of
  small ones: its files are five times smaller and show no seams at
  fractional sizes.

### Security

- An `<image>` in a skin's SVG can no longer read files on the computer;
  only pictures carried inside the SVG are drawn.

## [0.6.0] - 2026-11-07

### Added

- Skins and plug-ins are also read from `popamp/skins` and
  `popamp/plugins` in each of `$XDG_DATA_DIRS` (`/usr/share/popamp/…`),
  where their packages install them. Your own copy of the same name wins,
  and a packaged plug-in stays off until you allow its permissions.

### Changed

- The apt repository moved from GitHub Pages to https://apt.popamp.app,
  built and signed by [popamp/apt](https://github.com/popamp/apt) from
  the latest release of every popamp repository, so skins and plug-ins
  can join it as packages. Add it again with the commands in
  `docs/building.md`; the signing key stays the same.
- The `popamp-plus` deb provides `popamp` at its own version, so a
  package that needs a newer Popamp, such as a skin, installs next to
  either build.

## [0.5.0] - 2026-11-06

The first release of Popamp. It starts at 0.5.0 rather than 0.1.0, which
was Linamp's, since much has changed since then.

### Changed

- Building needs Rust 1.96 or newer (was 1.89): the plug-in host,
  wasmtime through Extism, needs it.
- YouTube left the core for a plug-in,
  [plugin-youtube](https://github.com/popamp/plugin-youtube), which needs
  a build with plug-ins. The settings `ytdlp_path`, `ffmpeg_path` and
  `ytdlp_format` move to `[plugin.youtube]` when the new version starts
  (only those changed from their defaults). A YouTube link without the
  plug-in fails with "no plug-in plays this link", as does any web page
  opened as a stream.
- Tracks open on a thread of their own. Stop, Next and Prev work while a
  stream is still connecting, instead of waiting for it (up to 20
  seconds for a silent server).
- Each location is played, listed and described by a source: files,
  HTTP streams, and plug-ins. A source names the program to run when a
  track plays through one, and the player runs it. A slow source shows
  "CONNECTING…" while it opens.
- A menu that does not fit below the mouse opens above it instead of
  over it, so a second click on the button that opened it closes it.
- The project is now Popamp, a separate project grown from
  [Linamp](https://github.com/sayoojkkarun/linamp) 0.1.0. The program,
  crates, packages and settings folders (`~/.config/popamp`,
  `~/.local/share/popamp`) carry the new name.

### Added

- Windows (x86_64). Releases have an installer and a portable zip of
  each build. The installer adds a Start menu entry, opens skins with
  Popamp and lists it in "Open with" for audio files and playlists;
  settings are in `%APPDATA%\popamp`, or, in the portable zip, beside
  `popamp.exe`. The program has the Popamp icon. Sound goes to the
  default device through WASAPI, fonts come from the system's font list,
  and files, folders and questions use Windows' own dialogs. Started
  from cmd or PowerShell, `/STATUS`, `/VERSION` and errors print in that
  console; started otherwise, no console window opens. Remote commands reach a
  running instance through a named pipe. A bar skin's strip is taken out
  of the work area, as the taskbar's is. The media keys and the player
  card by the volume flyout show the track and its cover, play, pause,
  stop, skip, seek, and switch shuffle and repeat.
- Media controls show the track's cover: the picture in its tags (the
  front cover first), else an image beside it named `cover`, `folder`,
  `front`, `album` or `albumart` (`.jpg`, `.jpeg`, `.png`). On Linux it is
  MPRIS's `mpris:artUrl`, for desktop widgets and `playerctl`.
- Two builds: `popamp`, the player, and `popamp-plus`, built with
  plug-ins. Releases have a deb and an AppImage of each; `popamp-plus`
  replaces `popamp` and the other way round. `/VERSION`, About and the
  title display say which one runs.
- Plug-ins: WebAssembly modules run by [Extism](https://extism.org), each
  in its own folder under `~/.local/share/popamp/plugins/` with a
  `plugin.toml`. A source plug-in takes links on the hosts its manifest
  names, lists them, describes them and tells the player how to play
  them; it can start only the programs its manifest names, and reach only
  the hosts it names. "Plug-ins…" in the Options menu shows what each one
  may do next to its check box, turns it on and edits its settings; a new
  version that asks for more stays off until it is allowed again. A
  plug-in that crashes or runs over 60 seconds three times in a row is
  switched off, and the title display and "Plug-ins…" say why. A dsp
  plug-in changes the sound after the equalizer, block by block, on the
  engine thread; a block that fails or is late
  plays unchanged. A track given at startup waits for the plug-ins to
  load (up to 30 seconds), so it goes to its plug-in. The calls are in the
  [`popamp-plugin-api`](https://github.com/popamp/popamp-plugin-api) crate and in
  [docs/plugins.md](docs/plugins.md). Plug-ins are built only with
  `cargo build --features plugins`; the default build is the player and
  its skins, without the WebAssembly runtime.
- [Popamp Steel](https://github.com/popamp/skin-steel), a silver and
  navy vector skin laid out like the "modern" players of the 2000s: a
  dotted blue display with a segmented analyzer, round transport buttons,
  toggles with blue lamps, an equalizer with a tab and a dB scale, and a
  shade mode for all three windows. Its texts use its own pixel fonts (`fonts/*.ttf`, Latin and
  Cyrillic).
- Bar skins: `placement = "bar-bottom"` (or `"bar-top"`) makes a vector
  skin's main window a strip across the screen at that edge, inside the
  desktop's own panels (it moves when they appear or resize). Its strip is
  reserved like a taskbar's, the playlist and the equalizer sit against it
  at the right end, and the main menu's "Bar position" moves it to the
  other edge. The bar has its own size, ×2 unless "Bar size" in the main
  menu says otherwise, whatever size the other windows are.
  [Popamp Bar](https://github.com/popamp/skin-bar) is Popamp Modern with
  a 14 pixel high bar, as high as a classic window shade.
- A marquee without `area` writes the title in all of its element, so it
  can stretch with a resizable window.
- `see_through = true` in a vector skin's window: the window shows only
  where its images are opaque (and its list, vis and EQ graph), so a
  resizable window can have rounded corners.
- [Popamp Modern](https://github.com/popamp/skin-modern), a silver
  vector skin with its own layout, round buttons that take clicks only on
  their shape (the new `hit` key) and rounded windows.
- Popamp's own skins other than the default one live in repositories of
  their own, each with the script that generates it. A skin folder skips
  hidden files, so a skin cloned into the skins folder with git works and
  a commit in it does not reload it.
- The default skin is a vector skin folder, `skins/default/` (SVG files and
  `skin.toml`), built into the program; it replaces the skin drawn in code.
  Vector skins take any sheet they leave out from it.
- `.psz`: Popamp's vector skin archive. Opening one installs it, like
  `.wsz`, and it has its own MIME type.
- `.deb` and AppImage packages for x86_64 and ARM64, built by a release
  workflow on version tags (`packaging/build-deb.sh`,
  `packaging/build-appimage.sh`), and AppStream metadata.
- rpm packages for openSUSE Tumbleweed and Leap 16.0 and Fedora 43 and
  44 (x86_64 and ARM64), built and published in a repository by the
  openSUSE Build Service (`packaging/obs/`), so they update with the
  system.
- An apt repository on GitHub Pages for Debian and Ubuntu, filled with
  each release's debs by the release workflow and signed
  (`packaging/apt/`), so Popamp updates with `apt upgrade`.
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

[Unreleased]: https://github.com/popamp/popamp/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/popamp/popamp/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/popamp/popamp/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/popamp/popamp/releases/tag/v0.5.0
[0.1.0]: https://github.com/sayoojkkarun/linamp/releases/tag/v0.1.0
