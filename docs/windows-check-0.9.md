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
   buttons. Pausing changes the icon's state at once. Code:
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

(date, Windows version, build or commit; per item: works / fails, with
what was seen)
