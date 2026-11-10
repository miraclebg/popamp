# Roadmap

What is left before 1.0, split into versions of about the same size.
Contributions are welcome. Please open an issue first for larger items.

## 0.8: Install and set up

- **Gallery in the player:** a window to browse and search the catalog on
  https://popamp.app (`/api/v1/catalog.json`) and install a skin or
  plug-in in a click, instead of opening a downloaded file or cloning a
  repository into the plug-ins folder. It shows a skin's preview, and
  what a plug-in may do before it is installed, checks each download
  against the catalog's checksum and offers updates. Windows has no
  packages, so there this is the way to install. The Skin Museum window,
  which does this for Winamp skins, shows the way.
- **Tags and art:** tag editor, and album art in the windows (the media
  controls already show it).
- **Skins and plug-ins as rpms** in the openSUSE Build Service, as they
  already are debs in https://apt.popamp.app.
- **Packages checked by hand:** install the rpms from the repository on
  openSUSE and Fedora. On a real Windows machine, what Wine cannot show:
  the bar's strip shrinking the work area, the media keys, and the
  timeline, shuffle, repeat and cover in the media overlay. The Windows
  installer: install for one user and for all, file types, upgrade,
  uninstall.

## 0.9: Sound and desktop

- **More formats:** Opus (libopus), MIDI (SoundFont synth), tracker modules
  (libopenmpt), CUE sheets.
- **Decoder plug-ins.**
- **Playback extras:** crossfade, a ReplayGain scanner, history (Ctrl+H).
- **Desktop integration:** tray icon (StatusNotifierItem), track-change
  notifications, global hotkeys through the XDG portal.
- **MilkDrop-compatible visualizer window** through libprojectM.

## 1.0: The whole player

- **Native Wayland mode:** draw all windows into one surface, since Wayland
  does not let apps position their own windows.
- **Local media library:** folder scanning, search, library playlists.
- **Modern skins:** Wasabi XML layout and the MAKI script VM.
- **Hands-on testing before the release:** mouse interaction, docking,
  menus, dialogs, drag & drop and double size across desktops (GNOME,
  KDE, Xfce, i3), distributions, and Windows 10 and 11.
