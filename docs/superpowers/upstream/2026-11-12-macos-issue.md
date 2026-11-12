I use a Mac and would like to use Popamp on it every day, so I would like to build the macOS port and contribute it here. Before sending any code, here is the plan, so you can say whether you want it and how.

## Approach

The same shape the code already has for Linux and Windows: each place that depends on the system gets a macOS module behind the interface it already has, next to `alsa.rs`/`wasapi.rs`, `x11.rs`/`windows.rs` and the others.

- Apple's frameworks through the `objc2` crates, which winit already brings in. Building needs only the Xcode Command Line Tools, no packages.
- A helper program where Linux uses one: `osascript` for dialogs, as `zenity` is used now.
- No change to the Linux or Windows code. The first step is a cfg split: today `cfg(unix)` means Linux, so macOS gets X11, ALSA and D-Bus and the build fails.

The baseline is macOS 12 or newer, Apple Silicon first, Intel later.

## Stages

Each stage is one small PR, or a few for stage 3. A macOS CI job comes with the first PR.

1. **It builds.** The cfg split; macOS modules that do nothing yet; a macOS job in CI.
2. **It plays.** Sound through CoreAudio, behind the same `Pcm` interface as ALSA and WASAPI.
3. **It is usable.** Correct size on Retina screens, Mac shortcuts, dialogs through `osascript`, system fonts through CoreText, settings in `~/Library/Application Support/popamp`, Opus and libopenmpt as `.dylib`s, and opening files from Finder (macOS sends those as Apple Events, which winit does not pass on).
4. **It can be downloaded.** `packaging/build-macos.sh` making `Popamp.app` and a `.dmg` for both editions; a release job; docs.
5. **Everything else.** Shaped skins (`region.txt`, through a transparent window), media keys and Now Playing, "Open with Popamp" in Preferences, keep-awake for visualizers, the bar skin, Intel builds.

## Mac conventions

On a Mac, Popamp should behave like a Mac app. ⌘ is the main modifier, Ctrl+click is a right click, and Popamp's Ctrl+click actions become ⌘+click. Most shortcuts just swap Ctrl for ⌘. These differ, because macOS or Mac habits already use them:

| Action | Linux/Windows | macOS |
|---|---|---|
| Quit | Alt+F4 | ⌘Q |
| Preferences | Ctrl+P | ⌘, |
| Close window / minimize | none | ⌘W / ⌘M |
| Windowshade | Ctrl+W | ⇧⌘W |
| Cycle Popamp's windows | Ctrl+Tab | ⌘` |
| Always on top | Ctrl+A | ⌥⌘A (⌘A selects all in the playlist) |
| File info / invert selection | Alt+3 / Ctrl+I | ⌘I / ⇧⌘I |
| Sort by title, file name, path | Ctrl+Shift+1/2/3 | ⌥⌘1/2/3 (⌘⇧3 is the system's screenshot) |
| Stop after current | Ctrl+V | ⌥⌘V |
| Paste links or files into the playlist | none | ⌘V (new) |
| Remove / crop / clear | Del / Ctrl+Del / Ctrl+Shift+Del | ⌫ / ⌘⌫ / ⇧⌘⌫ |
| Top / bottom of the playlist | Home / End | also ⌘↑ / ⌘↓ |

Linux and Windows keep their keys and labels.

## Questions for you

1. Do you want macOS as a supported platform? I plan to keep it working, and I'd like to know what you expect from whoever does.
2. The cfg split: is a build-script cfg (`linux_like`, every Unix but macOS) all right, or would you rather see `all(unix, not(target_os = "macos"))` written out?
3. Settings and skins in `~/Library/Application Support/popamp`?
4. A bundle identifier for `Popamp.app`, for example `app.popamp.Popamp`?
5. Signing and notarisation need an Apple Developer account ($99 a year), which belongs to whoever publishes releases. Until there is one, releases would be ad-hoc signed, with instructions for opening them past Gatekeeper.
6. Is a macOS CI job on every PR acceptable? macOS runners are slower.
7. The Mac keymap above, and ⌘V paste.

The first PR (stage 1) is nearly ready: it builds and passes the tests on macOS, with CI green on all three systems. I'll wait for your answer before sending it.
