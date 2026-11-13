# Third-party material

Popamp's own code is licensed under MIT OR Apache-2.0. Parts of it are
adapted from other projects under the same licences; they are listed here
with what was taken.

## Linamp

<https://github.com/sayoojkkarun/linamp>, Copyright (c) 2026 Sayooj K
Karun, MIT OR Apache-2.0.

Popamp started as a fork of Linamp 0.1.0 and was renamed when it became a
separate project. Everything up to that release is Linamp's code; the git
history keeps each change under its author.

## Ferramp

<https://github.com/SpankulatorX/ferramp>, Copyright (c) 2026 The Ferramp
contributors, MIT OR Apache-2.0.

| Popamp | Taken from Ferramp |
|---|---|
| `apps/popamp/src/headless.rs` (`/RENDER`) | the idea of headless rendering and its set of window states (`src/render.rs`); the code was written for Popamp's model |
| `popamp-skin/src/cursors.rs`, `cursor()` in the windows | the `.cur`/`.ani` decoding approach and which cursor shows over which control (`src/skin/mod.rs`, `src/ui/*_win.rs`) |
| `plformats::html` | the HTML playlist export and its colours (`src/app.rs` `html_playlist`) |
| EQ "Default" preset (`Settings::eq_default`) | the Load/Save Default entries (`src/app.rs` `eq_lib`) |
| bookmarks in `model.rs` | the feature, its menus and keys (`src/app.rs`, `src/ui/menu.rs`) |
| `widgets.rs`, `panels.rs` (own dialogs) | the idea of drawing dialogs from a list of simple controls, and Jump to file's word search (`src/ui/dialogs.rs`); the code was written for Popamp |
| `museum.rs`, `SkinMuseum` in `panels.rs` | the Skin Museum: GraphQL queries, reply parsing, safe file names and the dialog layout (`src/museum.rs`, `src/ui/dialogs.rs`) |
| `MainWin::tooltip`, tooltip timing in `app.rs` | the tooltip texts, the 0.6 s delay and 7 s timeout (`src/ui/main_win.rs`, `src/app.rs`) |
| `packaging/build-deb.sh`, `packaging/build-appimage.sh`, `.github/workflows/release.yml` | the package scripts and the release workflow, including the pinned appimagetool checksums (`packaging/linux/`, `.github/workflows/release.yml`) |

## Shipped with the Windows downloads

Linux takes these from the system; the Windows installer and zip carry
them beside `popamp.exe`, with their licences in `licenses/`
(`packaging/windows/fetch-libs.sh` fetches them).

| File | Project | Licence |
|---|---|---|
| `opus.dll` | [Opus](https://opus-codec.org), built by vcpkg | BSD-3-Clause |
| `libopenmpt.dll`, `openmpt-*.dll` | [libopenmpt](https://lib.openmpt.org), the official build, with mpg123, Ogg, Vorbis and zlib | BSD-3-Clause; LGPL-2.1 (mpg123); BSD (Ogg, Vorbis); zlib |
| `GeneralUser-GS.sf2` | [GeneralUser GS](https://www.schristiancollins.com/generaluser.php) by S. Christian Collins, the SoundFont for MIDI | GeneralUser GS License v2.0 (free to use and to ship in software) |

## Shipped with the macOS downloads

Popamp.app carries these in `Contents/Frameworks` and
`Contents/Resources`, with their licences in `Contents/Resources/licenses`
(`packaging/macos/fetch-libs.sh` builds the libraries from their source
releases for macOS 12).

| File | Project | Licence |
|---|---|---|
| `libopus.0.dylib` | [Opus](https://opus-codec.org) 1.5.2 | BSD-3-Clause |
| `libopenmpt.0.dylib` | [libopenmpt](https://lib.openmpt.org) 0.8.10, built without its mpg123 and Vorbis decoders | BSD-3-Clause |
| `GeneralUser-GS.sf2` | [GeneralUser GS](https://www.schristiancollins.com/generaluser.php) by S. Christian Collins, the SoundFont for MIDI | GeneralUser GS License v2.0 (free to use and to ship in software) |
