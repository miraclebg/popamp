# Windows check for 0.9

A hand check of the 0.9 work on a real Windows machine, written for the
person or agent who runs it. Everything below was built and tested on
Linux; on Windows only CI (clippy and tests) and earlier Proton runs have
seen it. Write the results at the end of this file and commit them.

## Get the build

Branch `playback-extras` holds all of 0.9 so far: formats (PR #30),
decoder plug-ins (PR #31) and the playback and desktop extras.

- Ready-made: the Release workflow, run by hand on the branch
  (`gh run list --repo popamp/popamp --workflow release.yml`), leaves the
  artifact `windows-x86_64` with the installers and portable zips of
  `popamp` and `popamp-plus`. Check `popamp-plus`; it has every feature.
- Or build it: `sh packaging/windows/fetch-libs.sh dist/libs` (needs vcpkg
  for `opus.dll`), then `cargo build --release -p popamp --features plus`
  and put the DLLs and `GeneralUser-GS.sf2` from `dist/libs` next to
  `popamp.exe`.

Settings live in `%APPDATA%\popamp\popamp.ini` (section `[Popamp]`). A
`popamp.ini` beside the exe makes the copy portable, and `/INIDIR=<dir>`
picks another folder, which keeps tests away from real settings. Set
`RUST_LOG=info` (or `debug`) to see the log on stderr. Preferences:
`Ctrl+P`.

Remote commands go to the running instance: `popamp.exe /PLAY`,
`/STARTMAX` (bring the windows back), `/COMMAND=40045` (Play),
`/COMMAND=50004` (Minimize), `/QUIT`.

## What to check

Only Linux has run these by hand unless marked otherwise.

1. **Opus** (`.opus`, and Opus in `.mka`/`.webm`): plays and seeks.
   Loads `opus.dll` beside the exe (`crates/popamp-audio/src/opus.rs`).
   Never run on Windows: Proton had no `opus.dll`.
2. **MIDI** (`.mid`, `.kar`): plays through the bundled
   `GeneralUser-GS.sf2`; a SoundFont set in Preferences takes over.
   Passed in Proton.
3. **Tracker modules** (`.mod .xm .s3m .it`): `libopenmpt.dll`. Passed in
   Proton.
4. **CUE sheets:** opening `album.cue` lists its tracks; seeking stays
   inside a track. Passed in Proton.
5. **Decoder plug-in:** install
   [plugin-ape](https://github.com/popamp/plugin-ape) in Options >
   Plug-ins…, allow it, then play a `.ape` file.
6. **Crossfade:** Preferences > "Crossfade (ms)", e.g. 3000. Two tracks
   of the same sample rate overlap at the change, both of one album and
   of different albums. A pair with different sample rates stays gapless.
   Check by ear. Code: `start_blend` in
   `crates/popamp-audio/src/engine.rs`.
7. **History** (`Ctrl+H`): plays show up, search works, and
   `history.txt` is in the settings folder.
8. **ReplayGain scanner:** select tracks, then *Scan ReplayGain* in the
   playlist's menu. Values go to `replaygain.txt`; the audio files must
   not change (compare their modified time and hash).
9. **Tray icon** (Preferences > Desktop > "Icon in the system tray", on by
   default): the icon is there. A left click brings Popamp up, a middle
   click plays or pauses, and the right-click menu has the transport
   buttons. Pausing changes the icon's tooltip at once; the picture is
   always the logo. Code:
   `apps/popamp/src/media/tray_windows.rs`.
10. **Minimize to the tray** (Desktop > "Minimize to the tray"):
    - With the icon setting **off** and this one on, minimizing hides all
      windows, including the taskbar button, and the icon appears. A
      click on the icon, or `/STARTMAX`, brings the windows back and the
      icon goes.
    - With both on, the icon stays all the time.
    - With only the icon on, minimize goes to the taskbar as usual.
    - `/STARTMIN` at start goes straight to the tray when the option is on.

    Code: `in_tray` and `raise()` in `apps/popamp/src/model.rs`,
    `wanted_visible` in `apps/popamp/src/app.rs`.
11. **Global hotkeys** (Desktop > "Global hotkeys", off by default). With
    another app focused:
    - Ctrl+Alt+Insert play, Home pause, End stop;
    - Page Up and Page Down for previous and next;
    - Up and Down for volume;
    - Left and Right for 5 seconds back and forward.

    A key another program already holds is skipped with a warning in the
    log. Code: `RegisterHotKey` in `apps/popamp/src/hotkeys/mod.rs`.
12. **Media overlay** (SMTC): title and cover change with each track, and
    pause shows at once. Never checked on real Windows: the timeline,
    shuffle, repeat and the media keys.
13. **Track notifications** exist on Linux only. On Windows the setting
    should do nothing and break nothing.

## Known

- Under Wine/Proton two tag tests fail with "Access denied": they replace
  a file that is still open. On real Windows they should pass. If they
  fail here too, that is a bug.
- `cargo test --workspace --features popamp/plus` must pass. CI runs it
  on Windows Server 2025.

## Results

2026-11-11, Windows 11 Home 10.0.26300. A local release build of
`playback-extras` with line tables (`--features plus`, version set to
0.9.0 for the build), at 83c72c1, then 447f92b, then 6188df9, each
started with `/NEW /INIDIR=<scratch>` beside the installed 0.8.

Three bugs were found and fixed on the branch:

- 83c72c1: Popamp stopped responding minutes after start, on Play or a
  drop from Explorer. The media controls' thread never answered window
  messages.
- 447f92b: after a pause, another track, a seek or Stop left the sound
  stuck, and closing hung. Emptying the WASAPI output kept it paused.
- 6188df9: the media card kept the previous cover for a track without
  one. Windows ignores an empty thumbnail; such a track now shows the
  logo.

Per item:

1. **Opus:** works (`.opus`, `.webm`).
2. **MIDI:** works.
3. **Tracker modules:** works (`.mod`).
4. **CUE sheets:** works.
5. **Decoder plug-in:** works once installed and allowed; `.ape` plays
   with the right durations. Seeking not checked: the test files are a
   few seconds long. Without the plug-in Symphonia takes `.ape` for
   something else and plays noise. plugin-ape asks for Popamp 0.9.0, so
   it installs only once the version is bumped.
6. **Crossfade:** works by ear.
7. **History:** works: plays listed, search, `history.txt` written.
8. **ReplayGain scanner:** works; all six tracks in `replaygain.txt`,
   fast. The audio files did not change.
9. **Tray icon:** works. A left click raises all three windows, a middle
   click plays or pauses, the menu has Show, Exit and the transport.
   Pause changes the tooltip; the picture stays the logo by design.
10. **Minimize to the tray:** works in all four cases, `/STARTMIN`
    included (all windows hidden at start).
11. **Global hotkeys:** works; every key, with another app focused.
12. **Media overlay:** works after the two fixes above: title and cover
    follow the track, a track without a cover shows the logo, and the
    card's previous, play/pause and next work while Popamp is
    minimized. This Windows 11 card shows no timeline and no shuffle or
    repeat buttons, so those were not checked; no media keys on the
    keyboard either.
13. **Track notifications:** works: turned on, nothing pops up, playback
    goes on.

Also seen: `%TEMP%` holds a dozen `popamp-cover-<pid>-<id>.jpg` left by
earlier runs. A tagged cover is written there for the media card and
removed on the next track, but not when Popamp exits. Not fixed yet.
