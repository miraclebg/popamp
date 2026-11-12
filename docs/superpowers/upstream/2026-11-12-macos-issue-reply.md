Thank you, that all works for me. In order:

1. **Community-maintained, experimental:** agreed. I'll keep macOS building and take the macOS bug reports. The release workflow I'll propose doesn't wait on macOS: Linux and Windows publish when they pass, and the disk images come along only if the macOS job passed too.
2. **`linux_like`:** one build-script cfg, declared with `cargo::rustc-check-cfg`. Plain `unix` stays only where the code really is POSIX (the control socket, `sh` pipes, symlinks).
3. **Settings:** `~/Library/Application Support/popamp`, with `/INIDIR` and the portable `popamp.ini` still taking precedence, as now.
4. **Keymap:** the differences live in one table, `apps/popamp/src/mac_keys.rs`. The key handler and the menu labels both read from it, and `docs/usage.md` lists the same table and points to it.
5. **Signing:** nothing depends on it. `packaging/build-macos.sh` signs ad hoc by default. With a Developer ID certificate and an App Store Connect key in the repository's secrets, the release job signs, notarises and staples, in this repository's CI and from the tag. Whose identity that is stays your call; until then the docs say how to open an unsigned build the first time.
6. **CI:** GitHub's hosted runners only, with the macOS job running after merges to `main`.

**Timing:** understood. I'll wait for 0.9, rebase onto `main`, and give the new tray, global-hotkey and media-control modules their macOS placeholders in the cfg split. Then I'll send stage 1 on its own.

Meanwhile I built every stage in my fork, so the PRs you get will have been used for a while. Everything runs on an Apple Silicon MacBook Pro with a second, non-Retina screen: CoreAudio playback and device switching, the Mac dialogs, CoreText fonts, opening files from Finder, the Mac keys, shaped skins, and a disk image built in CI that installs and plays Opus, tracker modules and MIDI on its own. Linux and Windows CI stayed green throughout.

Two things I noticed in passing that are yours, not macOS's:

- The release workflow's Linux jobs don't install `libopus0` and `libopenmpt0`, which the Opus and tracker tests need since #30. Your `playback-extras` branch already does, so it should go away with that.
- On Windows, `WasapiPcm` seems to keep its paused state across `discard()`. On macOS the same sequence (pause, then open another track at the same sample rate) failed until `discard` ended the pause, as ALSA's drop and prepare do. Worth a quick try on Windows: pause, then double-click another track.

I'll post here when stage 1 is ready, after 0.9.

Martin
