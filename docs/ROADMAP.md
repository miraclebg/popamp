# Roadmap

Planned work, roughly in priority order. Contributions are welcome. Please
open an issue first for larger items.

## Next

- **Hands-on testing:** verify mouse interaction, docking, menus, dialogs,
  drag & drop and double size across desktops (GNOME, KDE, Xfce, i3) and
  distributions.
- **Golden-image tests in CI:** render windows with `POPAMP_SNAPSHOT` under
  Xvfb and compare against committed PNGs.
- **Non-blocking YouTube startup:** resolve with `yt-dlp` off the audio
  thread so Stop works while connecting.

## Planned

- **Preferences window:** output device and file-type registration (the
  window itself, with buffer size, playlist font and title format, exists).
- **Tags and art:** tag editor and album art (read/write through a tagging
  crate).
- **More formats:** Opus (libopus), MIDI (SoundFont synth), tracker modules
  (libopenmpt), CUE sheets.
- **Playback extras:** crossfade, a ReplayGain scanner, bookmarks and
  history (Ctrl+H).
- **Desktop integration:** tray icon (StatusNotifierItem), track-change
  notifications, global hotkeys through the XDG portal.
- **Native Wayland mode:** draw all windows into one surface, since Wayland
  does not let apps position their own windows.

## Later

- MilkDrop-compatible visualizer window through libprojectM.
- Local media library (folder scanning, search, library playlists).
- Modern skins (Wasabi XML layout and the MAKI script VM).
- A dynamic plug-in ABI for third-party decoders and effects.
