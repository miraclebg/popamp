# Third-party material

Linamp's own code is licensed under MIT OR Apache-2.0. Parts of it are
adapted from other projects under the same licences; they are listed here
with what was taken.

## Ferramp

<https://github.com/SpankulatorX/ferramp>, Copyright (c) 2026 The Ferramp
contributors, MIT OR Apache-2.0.

| Linamp | Taken from Ferramp |
|---|---|
| `apps/linamp/src/headless.rs` (`/RENDER`) | the idea of headless rendering and its set of window states (`src/render.rs`); the code was written for Linamp's model |
| `linamp-skin/src/cursors.rs`, `cursor()` in the windows | the `.cur`/`.ani` decoding approach and which cursor shows over which control (`src/skin/mod.rs`, `src/ui/*_win.rs`) |
| `plformats::html` | the HTML playlist export and its colours (`src/app.rs` `html_playlist`) |
| EQ "Default" preset (`Settings::eq_default`) | the Load/Save Default entries (`src/app.rs` `eq_lib`) |
| bookmarks in `model.rs` | the feature, its menus and keys (`src/app.rs`, `src/ui/menu.rs`) |
| `widgets.rs`, `panels.rs` (own dialogs) | the idea of drawing dialogs from a list of simple controls, and Jump to file's word search (`src/ui/dialogs.rs`); the code was written for Linamp |
| `museum.rs`, `SkinMuseum` in `panels.rs` | the Skin Museum: GraphQL queries, reply parsing, safe file names and the dialog layout (`src/museum.rs`, `src/ui/dialogs.rs`) |
| `MainWin::tooltip`, tooltip timing in `app.rs` | the tooltip texts, the 0.6 s delay and 7 s timeout (`src/ui/main_win.rs`, `src/app.rs`) |
| `packaging/build-deb.sh`, `packaging/build-appimage.sh`, `.github/workflows/release.yml` | the package scripts and the release workflow, including the pinned appimagetool checksums (`packaging/linux/`, `.github/workflows/release.yml`) |
