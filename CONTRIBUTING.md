# Contributing to Popamp

Thanks for helping! Bug reports, testing on other distributions and pull
requests are all welcome.

## Reporting bugs

Please include:

- distribution, desktop and session type (`echo $XDG_SESSION_TYPE`)
- `popamp /VERSION` and how you installed it
- steps to reproduce, and what you expected instead
- a log from `RUST_LOG=debug popamp /NEW …` if playback or skins misbehave
- for skin problems, the `.wsz` file (if you are allowed to share it)

## Development setup

Requirements are in [docs/building.md](docs/building.md#requirements): Rust 1.96 or
newer and a C compiler. No `-devel` packages are needed.

```sh
cargo build
cargo run -p popamp -- /NEW /INIDIR=/tmp/popamp-dev ~/Music
```

Use `/NEW /INIDIR=<dir>` while developing so you don't touch your real
settings or a running instance.

Useful tools:

```sh
cargo run -p popamp-audio --example play -- --vol 0 file.flac   # engine without UI
cargo run -p popamp-skin --example dump_sheets -- skin.wsz out/  # skin sheets as PNG
POPAMP_SNAPSHOT=out/ cargo run -p popamp -- /NEW /INIDIR=/tmp/x  # windows as PNG, then exit
cargo run -p popamp -- /RENDER=out/ /SKIN=skin.wsz                 # same, headless, fixed states
```

`/RENDER` needs no X server or sound card. It writes one PNG per window and
state (stopped, playing with spectrum or oscilloscope, toggles, equalizer
values and a preset, windowshade, small and wide playlist) using the
settings in `out/popamp.ini` if that file exists.

## Code layout

| Crate | Contents |
|---|---|
| `crates/popamp-core` | settings, playlist model and formats, title formatting, EQF, command ids |
| `crates/popamp-audio` | decoding, EQ, limiter, ALSA and WASAPI output, engine thread, sources, streams, covers |
| `crates/popamp-skin` | skin loading, sprite geometry, default skin generator |
| `apps/popamp` | windows, menus, input, dialogs, IPC, media controls (MPRIS, SMTC) |

What was tested and how is in [docs/testing.md](docs/testing.md); its
"Not yet tested by hand" list is a good place to start.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks, again with `--features popamp/plugins` and with
`--features popamp/plus`, plus a build with the minimum supported Rust
version (1.96).

## Repositories

The name of a repository in the [popamp](https://github.com/popamp)
organization says whether it needs Popamp to be rebuilt:

- `popamp-<name>`: built into the program, even when it is optional, such
  as [popamp-plugin-api](https://github.com/popamp/popamp-plugin-api), the
  crate behind `--features plugins`.
- No prefix: added to an installed Popamp without rebuilding it, such as
  `plugin-<name>` (plug-ins, [plugin-youtube](https://github.com/popamp/plugin-youtube))
  and `skin-<name>` (skins, [skin-steel](https://github.com/popamp/skin-steel)).

Guidelines:

- **Scope:** keep changes focused; one logical change per commit, with a
  message that says why.
- **Tests:** add unit tests for parsing, formats and model logic. UI changes
  should come with before/after `/RENDER` images in the PR.
- **Real-time safety:** don't block the audio thread on locks the UI holds
  for long, and don't do I/O on the UI thread. Use the existing worker
  threads and events.
- **Artwork:** do not add artwork, presets or code from Winamp or other
  proprietary software. The default skin is Popamp's own vector skin in
  `skins/default/` (built into the program), and third-party skins are
  loaded at runtime, never bundled.
- **Dependencies:** must be compatible with MIT OR Apache-2.0 distribution.
  Prefer crates that don't need system `-devel` packages at build time.

## License

By contributing you agree that your contributions are dual licensed under
MIT OR Apache-2.0, as described in the [README](README.md#license).
