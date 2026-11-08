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
- Install the rpms from the repository on openSUSE and Fedora. (The debs
  from https://apt.popamp.app install and start on Debian 12 and 13 and
  Ubuntu 22.04, 24.04 and 26.04, x86_64 and ARM64; popamp/apt's Test
  install workflow checks that daily.)

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
- **Catalog on the website:** one catalog of Popamp skins and plug-ins
  on the Popamp website, to which anyone with an account can add their
  own. Each entry must link to its git repository; the site keeps the
  files (`.psz`, plug-in folders) and the skin previews, and publishes
  the catalog as JSON for the player.
- **Gallery in the player:** a window to browse and search that catalog
  and install a skin or plug-in in a click, instead of opening a
  downloaded file or cloning a repository into the plug-ins folder. It
  shows a skin's preview, and what a plug-in may do before it is
  installed, checks each download against the catalog's checksum and
  offers updates. Needs the catalog; the Skin Museum window, which does
  this for Winamp skins, shows the way.
- **Skins and plug-ins as packages:** `apt install popamp-skin-<name>`
  or `popamp-plugin-<name>` (and the same as rpms), for skins and
  plug-ins we have checked. The repositories keep their names
  (`skin-<name>`, `plugin-<name>`); only the packages carry the
  `popamp-` prefix.

  Done for apt: Popamp reads `/usr/share/popamp/skins` and `…/plugins`
  (all of `XDG_DATA_DIRS`) after the user's folder, whose copy wins; a
  packaged plug-in stays off until allowed. popamp/apt's `addon-release`
  workflow, called by each skin and plug-in repository on a `v<version>`
  tag, builds an `Architecture: all` deb (a skin depends on
  `popamp (>> 0.5.0)`, a plug-in on `popamp-plus (>> 0.5.0)` and
  recommends the programs it runs) and adds it to apt.popamp.app, which
  is rebuilt from every repository's latest release. Since Popamp 0.6.0
  the three skins and two plug-ins are there at 1.0.0, and their READMEs
  give the `apt install` line.

  To do: the same as rpms in the openSUSE Build Service.

  Windows has no packages, so the gallery is still needed.
- **Native Wayland mode:** draw all windows into one surface, since Wayland
  does not let apps position their own windows.

## Later

- MilkDrop-compatible visualizer window through libprojectM.
- Local media library (folder scanning, search, library playlists).
- Modern skins (Wasabi XML layout and the MAKI script VM).
- Decoder plug-ins.
