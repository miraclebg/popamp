# Roadmap

Planned work, roughly in priority order. Contributions are welcome. Please
open an issue first for larger items.

## Next

- **Hands-on testing:** verify mouse interaction, docking, menus, dialogs,
  drag & drop and double size across desktops (GNOME, KDE, Xfce, i3),
  distributions, and Windows 10 and 11.
- **Golden-image tests in CI:** render windows with `POPAMP_SNAPSHOT` under
  Xvfb and compare against committed PNGs.

## First release

The first release, 0.5.0, ships packages (deb and AppImage for Linux,
an installer and a portable zip for Windows) and a Linux package
repository. Two builds:

- **popamp:** the player without plug-in support.
- **popamp-plus:** the same player built with `--features plugins`.

Done: both are built, tested and packaged by the release workflow: deb
and AppImage for x86_64 and ARM64, where `popamp-plus` provides,
conflicts with and replaces `popamp`, and an installer and a portable
zip for Windows x86_64. Popamp runs on Windows with sound, fonts,
dialogs, remote control, the bar's strip and the media controls. rpms
for openSUSE and Fedora are built by the openSUSE Build Service
(`packaging/obs/`) and published in its repository.

To do:

- Check on a real Windows machine what Wine cannot show: the bar's strip
  shrinking the work area, the media keys, and the timeline, shuffle,
  repeat and cover in the media overlay.
- Try the Windows installer: install for one user and for all, file
  types, upgrade, uninstall.
- Install the rpms from the repository on openSUSE and Fedora.

Until then version numbers stay as they are, including the plug-in API
(1.0), and change in place; they start to move with this release.

## Planned

- **Preferences window:** output device, picked by name on ALSA and
  WASAPI alike (both play on the default device now), and file-type
  registration, on Linux and Windows (the window itself, with buffer
  size, playlist font and title format, exists).
- **Tags and art:** tag editor, and album art in the windows (the media
  controls already show it).
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
- Decoder and visualizer plug-ins.
