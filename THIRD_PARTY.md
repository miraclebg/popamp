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
| `packaging/build-deb.sh`, `packaging/build-appimage.sh`, `.github/workflows/release.yml` | the package scripts and the release workflow, including the pinned appimagetool checksums (`packaging/linux/`, `.github/workflows/release.yml`) |
