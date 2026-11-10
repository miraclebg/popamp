# Features


- **Classic windows:** Main, Equalizer and Playlist Editor at any size from
  x1 to x3 in steps of 0.25 (`Ctrl+D` switches between x1 and the chosen
  size). Classic skins stay crisp at fractional sizes. Windows dock and snap together, and
  support windowshade mode, EasyMove, always-on-top and `region.txt` shapes.
  Tooltips explain the main window's buttons.
- **Own dialogs:** Jump to file (search as you type), File info and a
  Preferences window (`Ctrl+P`). Opening and saving files uses the desktop's
  dialog: `zenity` or `kdialog` on Linux, the system's own on Windows.
- **Vector skins** (`.psz`): drawn in SVG, sharp at every size, with TrueType text
  and live reload while you edit them. Popamp's default skin is one. See
  [skin-format.md](skin-format.md).
- **Classic skins:** `.wsz`/`.zip` archives or unpacked directories. Each file
  falls back to the default skin if missing or broken. `viscolor.txt`,
  `pledit.txt` and the skin's mouse cursors (`*.cur`) are supported.
  Skins > Skin Museum browses, searches and downloads skins from
  [skins.webamp.org](https://skins.webamp.org); Popamp contacts the
  museum only while that window is open.
- **Gallery:** Skins > Gallery (also under Options) lists the skins and
  plug-ins on [popamp.app](https://popamp.app) with a preview and what
  each plug-in may do, and installs or updates one in a click. Each
  download is checked against the catalog's size and SHA-256; what the
  gallery did not install itself is left alone. The build without
  plug-ins lists only the skins. Popamp contacts the site only while
  that window is open.
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
- **Plug-ins** (in a build with `--features plugins`): WebAssembly
  sources such as [YouTube](https://github.com/popamp/plugin-youtube) and
  sound effects such as [Crossfeed](https://github.com/popamp/plugin-crossfeed),
  each allowed by you in Options > Plug-ins…. In a build with
  `--features visualizers`, also GPU visualizers such as
  [Spectrum](https://github.com/popamp/plugin-spectrum), in a window that
  can go full screen. See [plugins.md](plugins.md).
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
  - Media keys and the desktop's player controls, with the track's cover:
    MPRIS2 on Linux (desktop widgets, `playerctl`), the media overlay on
    Windows.
  - Bar skins reserve their strip of the screen, like a taskbar.

## Known limitations

- Opus, MIDI and tracker modules are not supported.
- No native Wayland windows: Popamp needs Xwayland on Wayland desktops.
- The Preferences window (`Ctrl+P`) covers the common options; the rest are
  in the menus and `popamp.ini`.
- No tag editing, album art in the windows, tray icon, media library or Modern (`.wal`)
  skins yet. See [ROADMAP.md](ROADMAP.md).

