# Popamp on macOS: design

Date: 2026-11-12. Branch `macos-port-design` in the fork `miraclebg/popamp`.
This document stays in the fork; it is not part of any upstream pull request.

## Intent

Make Popamp a native macOS application and contribute the port to the
upstream project (`popamp/popamp`) so it stays there.

The contributor is a macOS user and wants to use Popamp daily on their own
Mac. Their use decides the order of the work:

- **Classic skins** (`.wsz`), "classic all the way".
- **Internet radio**, mostly opened from downloaded `.pls` files,
  sometimes from a pasted direct link.

Success:

1. The contributor plays radio and local files with classic skins on an
   Apple Silicon Mac, opening `.pls` files from Finder. This is the first
   milestone and should come early.
2. Upstream accepts the port as a series of reviewed pull requests, and
   macOS stays working because CI checks it.
3. Later, macOS gets everything Linux and Windows have.

## Goals, non-goals, anti-goals

**Goals**

- Build, lint and test on macOS in CI across all three feature sets
  (default, `popamp/plugins`, `popamp/plus`).
- Native sound output, correct size on Retina screens, Mac dialogs, Mac
  system fonts, Mac file locations, ⌘ shortcuts, opening files from Finder.
- A `Popamp.app` users can download.
- Eventually: shaped classic skins (`region.txt`), media keys and Now
  Playing, "Open with Popamp" defaults, keep-awake for visualizers, the
  bar skin.

**Non-goals (for now)**

- Intel Macs and universal binaries. Apple Silicon first; Intel comes with
  or after the packaging work.
- macOS older than 12.
- The Mac App Store, and the sandbox that comes with it.
- Drawing Popamp's menus with the native menu bar (`NSMenu`). Popamp keeps
  its own skin-drawn menus, as on the other systems.
- Codec gaps that exist on every platform, for example any radio stream
  format the decoders cannot read on Linux either.

**Anti-goals: failure even if it works**

- Changing behaviour on Linux or Windows, or adding a build-time system
  requirement there (the project builds with no `-devel` packages).
- One large pull request, or a long-lived branch that drifts from a project
  merging several PRs a day.
- Mac code in a different style from the existing platform code, or Mac
  special cases leaking into shared code where a platform module would do.
- Bundling Winamp artwork or anything proprietary (CONTRIBUTING.md).

## Constraints

From the upstream project (CONTRIBUTING.md, CI):

- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace`, each with default features, `popamp/plugins`
  and `popamp/plus`. Minimum Rust version 1.96 (`cargo check` with 1.96).
- One logical change per commit, with a message that says why. UI changes
  come with before/after `/RENDER` images.
- Dependencies must be MIT OR Apache-2.0 compatible. Avoid build-time
  system packages. Apple's frameworks come with the Xcode Command Line
  Tools, which are fine.
- Real-time safety: the audio thread never waits on locks the UI holds for
  long; the UI thread does no I/O.
- House style: plain-English doc comments that say what something is for,
  short modules per platform behind one shared interface.

Platform:

- macOS 12 or newer, Apple Silicon (`aarch64-apple-darwin`) first.
- AppKit wants its windows and most UI calls on the main thread. winit
  already runs the event loop there.
- macOS lets no application reserve screen space the way X11 struts and
  Windows app bars do.

## How the code is organised today

Each OS-dependent area has one interface and one module per system, chosen
with `cfg`:

| Area | Interface | Linux | Windows |
|---|---|---|---|
| Sound output | `popamp-audio/src/output/mod.rs`: `Pcm::{open, write, delay, pause, discard, drain}`, `available()`, `devices()` | `alsa.rs` (dlopen `libasound.so.2`) | `wasapi.rs` |
| Windows | `apps/popamp/src/platform/mod.rs`: `attributes(role)`, `Platform::{connect, set_shape, reserve, attach, no_activate, release, work_areas}`, `OWNED_FOLLOW_OWNER` | `x11.rs` | `windows.rs` |
| Dialogs | `dialogs/mod.rs`: `execute(&Request) -> Vec<String>`, run off the UI thread | `zenity.rs` (subprocess) | `windows.rs` |
| Fonts | `font.rs`: `system::family(name)`, `system::for_char(c)` → font file paths | `font/fontconfig.rs` (`fc-match`) | `font/windows.rs` (registry) |
| Media controls | `media/mod.rs`: `service::spawn(proxy, shared, instance) -> Option<Sender<Snapshot>>` | `mpris.rs` (zbus) | `smtc.rs` |
| Default player | `filetypes.rs`: `Desktop::{from_env, defaults, set}` | `mimeapps.list` | opens Settings |
| Keep awake | `keep_awake.rs`: `os::hold() -> Option<Hold>` | D-Bus ScreenSaver | `SetThreadExecutionState` |
| IPC | `ipc.rs`: `transport` | Unix socket | named pipe |
| Paths | `popamp-core/src/config.rs`: `Paths::system()` | XDG | `%APPDATA%` |

**The blocker:** the code uses `cfg(unix)` to mean Linux. macOS is also
`unix`, so it gets the X11, ALSA, D-Bus, fontconfig and zenity modules, and
`apps/popamp/Cargo.toml` adds `x11rb` and `zbus` for every Unix.
`platform/x11.rs` imports `winit::platform::x11`, which does not exist on
macOS, so the build fails.

Already portable, no work expected: decoding, DSP, streams (`ureq` with
`rustls` and bundled `webpki-roots`, so radio needs nothing from the OS),
skins, playlist, config format, Extism/wasmtime plug-ins, the software
renderer (`softbuffer` and winit have macOS backends with no extra
features), the IPC Unix socket. `Cargo.lock` already contains the `objc2`
family (pulled in by winit) and wgpu's Metal backend (`objc2-metal`,
`raw-window-metal`).

## Approaches considered

**A. Hand-written macOS modules in the existing pattern (chosen).**
A `macos.rs` (or similarly named) module next to the Linux and Windows ones
in each area, behind the same interface. System frameworks through the
`objc2` crate family (`objc2-core-audio`, `objc2-audio-toolbox`,
`objc2-app-kit`, `objc2-core-text`, `objc2-media-player`…), which winit
already brings in. Where Linux runs a helper program, macOS does too
(`osascript` in place of `zenity`). Linux and Windows code is untouched.
Most code to write, with CoreAudio the fiddliest, but it is the shape the
maintainers already review and maintain.

**B. Cross-platform crates on macOS only** (`cpal`, `rfd`, `souvlaki`).
Rejected. `cpal` is callback-driven while Popamp's engine pushes audio and
blocks, so an adapter buffer is needed anyway. `rfd` needs the main thread
on macOS, while Popamp runs dialogs off it. And one platform would be built
in a different style from the other two.

**C. Move every platform onto cross-platform crates.** Rejected. `cpal`
needs ALSA headers at build time on Linux, breaking the no-`-devel` rule,
and it replaces Linux and Windows code that works. Unlikely to be accepted.

The contributor chose **A**.

## Decided

- Approach A.
- Apple Silicon first; macOS 12 or newer.
- Contribute upstream as a staged series of small PRs, opening an upstream
  issue with this plan **before** the first PR.
- Priority follows the contributor's use: classic skins and radio, `.pls`
  opened from Finder, direct links through Open URL.
- Media keys and Now Playing come in the polish stage (the contributor did
  not object).
- **macOS first.** Popamp on a Mac follows Mac conventions, not the Linux
  or Windows ones: ⌘ is the primary modifier, with the keymap in Stage 3,
  item 2.
- **⌘V pastes** copied links or files into the playlist (new behaviour,
  for direct radio links).
- **If upstream declines or goes quiet,** the port continues in the fork as
  its own macOS edition. The fork then publishes its own macOS releases
  (Stage 4) and follows upstream by rebasing.

## The design

### Stage 0: upstream issue

Open an issue on `popamp/popamp` proposing macOS support: the stages below,
approach A, the macOS 12 / Apple Silicon baseline, a macOS CI job, and the
questions only the maintainers can answer (listed at the end). Wait for a
response before sending PR 1 upstream; adjust this design to what they say.

*My call (process):* the work does not wait for upstream review. It is
built as a stack of branches in the fork, one per future PR, each on top
of the last. The contributor uses the top of the stack daily. PRs go
upstream one at a time, and the stack is rebased on `upstream/main` often
so it never drifts far.

### Stage 1: it builds (PR 1)

- **Split `cfg(unix)`** into Linux-like systems and macOS. Keep plain `unix`
  only where the code is truly POSIX: Unix sockets, `sh` pipes, symlinks,
  file semantics in tests. Everything X11, ALSA, D-Bus, fontconfig, zenity,
  XDG or `mimeapps.list` gets the Linux-like cfg. *Builder's call:* an alias
  through the `cfg_aliases` build-dependency (small, MIT/Apache, already
  used by winit) or spelled-out `all(unix, not(target_os = "macos"))`; the
  alias name is the builder's, to be checked with the maintainers.
- `apps/popamp/Cargo.toml`: `x11rb` and `zbus` move to a Linux-like target
  section; macOS gets its own section as modules need it.
- **macOS stubs** so the program starts: `platform` with plain windows and
  no-op `set_shape`/`reserve`/`attach`; sound output that only offers the
  `null` device (so tests run); no dialogs, no system fonts, no media
  service. Each stub logs once that the feature is not there yet, as the
  Linux code does when a helper is missing.
- **Tests:** tests that use ALSA devices (`plug:null` in
  `popamp-audio/src/engine.rs`) are gated to Linux-like systems, or made to
  use a second device name the macOS backend accepts.
- **CI:** a `macos` job on an Apple Silicon runner (`macos-15`), matching
  the Windows job (clippy and tests, all three feature sets), run only
  after a merge to `main`, not on pull requests.
  `brew install opus libopenmpt` for the tests that load them. Because
  those tests load the libraries, the macOS library names and lookup
  (Stage 3, item 6) move into this PR.
- No behaviour change on Linux or Windows; the existing CI proves it.

### Stage 2: it plays (PR 2)

**`popamp-audio/src/output/coreaudio.rs`**, implementing the `Pcm` interface:

- An output AudioUnit (default output, or the HAL output unit bound to a
  chosen device) with a render callback.
- The callback reads from a single-producer, single-consumer ring buffer of
  `f32` frames sized from `latency_ms`. It takes no locks and allocates
  nothing, and on underrun it plays silence. *Builder's call:* a small
  audited crate (for example `rtrb`, MIT/Apache) or a hand-written atomic
  ring.
- `write` blocks until the ring has room (woken by the callback through a
  semaphore or condition variable the callback only signals).
- `delay` = frames in the ring plus the device's latency, safety offset and
  buffer size, as read from its properties.
- `pause(on)` stops or starts the unit and keeps the ring. `discard` empties
  the ring. `drain` waits until the ring is empty and the device latency has
  passed.
- The stream format is the track's rate and channel count in `f32`; the
  AudioUnit converts to the hardware rate.
- `devices()`: output devices from the HAL. `name` is the device UID
  (stable across reboots and what the settings keep); `label` is its name.
  `default` follows the system default output; `null` opens nothing, as on
  Windows.
- `available()` reports whether a default output device exists.

From here files and radio both play, started from the command line.

**Shaped-skin spike (right after Stage 2, throwaway).** Popamp draws every
window on the CPU into a pixel buffer; on macOS `softbuffer` hands it to
Core Animation (which uses Metal internally; Popamp itself uses Metal only
for visualizers, through wgpu). macOS has no call that cuts a window to a
shape, as X11 SHAPE and `SetWindowRgn` do. Instead: a transparent window
(winit `with_transparent(true)`), and pixels outside the `region.txt` mask
drawn with alpha 0. AppKit passes clicks through fully transparent pixels
and fits the shadow to the shape (after `invalidateShadow`). `softbuffer`
probably drops alpha on macOS; if so, macOS presents the buffer itself as a
premultiplied-alpha `CGImage` set as the window layer's contents. The spike
tries one shaped `.wsz` skin. *My call:* if it works, shaped skins move
into Stage 3 (classic skins are the contributor's priority); if not, they
stay in Stage 5 and the spike's findings go into this document.

**Spike result (2026-11-12, on the contributor's Mac).** Reference code:
`docs/superpowers/spikes/2026-11-12-macos-shapes.patch` (this branch only).
- **`softbuffer` drops alpha on macOS.** It presents with `CGImageAlphaInfo::NoneSkipFirst`, so it cannot make pixels transparent.
- **A mask layer works without touching presentation.** `set_shape` sets the view layer's `mask` to a `CALayer` whose contents is a premultiplied-alpha `CGImage` built from `shape_rects`. Windows are created with `with_transparent(true)`. Apart from that, nothing outside `platform/macos.rs` changes.
- **Seen:** an exact ellipse with the desktop showing around it; dragging works; the shadow appears to follow the shape, faintly.
- **Not working: clicks outside the shape still reach the window.** AppKit does not pass them through a masked layer. The likely fix is to poll the cursor position while it is over a shaped window's rectangle and turn the window's hit-testing off outside the mask (winit's `set_cursor_hittest`, i.e. `ignoresMouseEvents`). That still needs proving.
- **Decision (the spec's rule):** shaped skins move into Stage 3, as their own PR after the daily-use items. Click-through is part of that PR.

### Stage 3: daily use (several small PRs)

1. **Retina size.** Popamp sizes windows in physical pixels (`zoom` percent
   of skin pixels, `PhysicalSize` in `app.rs`). On a Retina screen (backing
   scale 2) the default zoom of 200% comes out at Winamp's 1× size.
   *My call:* on macOS `zoom` counts in points, so physical scale =
   zoom × backing scale. A nice side effect: zoom 100% and 150% become ×2
   and ×3 physical pixels, so classic bitmaps stay pixel-exact. The macOS
   default zoom is tuned by eye on a real Mac during this PR. Windows
   follow `ScaleFactorChanged` when moved between a Retina and a non-Retina
   monitor. Dialogs and menus already use `scale_factor()`
   (`dialog_pct`); check that they look right.
2. **Mac keymap (decided: macOS first).** ⌘ is the primary modifier and
   menus show ⌘ labels. Ctrl+Click is a right click, as Mac users expect;
   Popamp's Ctrl+Click actions become ⌘+Click. Shortcuts that do not clash
   just swap Ctrl for ⌘ (⌘O, ⌘S, ⌘L, ⌘N, ⌘D, ⌘E, ⌘J, ⌘T, ⌘R, ⌘B, ⌘Z…).
   These change:

   | Action | Linux/Windows | macOS |
   |---|---|---|
   | Quit | Alt+F4 | ⌘Q |
   | Preferences | Ctrl+P | ⌘, |
   | Close window / minimize | none | ⌘W (as its close button) / ⌘M |
   | Windowshade | Ctrl+W | ⇧⌘W |
   | Cycle Popamp's windows | Ctrl+Tab | ⌘` |
   | Always on top | Ctrl+A (main), Ctrl+Alt+A (playlist) | ⌥⌘A everywhere; ⌘A selects all in the playlist |
   | File info / invert selection | Alt+3 / Ctrl+I | ⌘I / ⇧⌘I |
   | Sort by title, file name, path | Ctrl+Shift+1/2/3 | ⌥⌘1/2/3 (⌘⇧3 is the system screenshot) |
   | Stop after current | Ctrl+V | ⌥⌘V |
   | Paste links or files into the playlist | none | ⌘V (new) |
   | Remove / crop / clear | Del / Ctrl+Del / Ctrl+Shift+Del | ⌫ / ⌘⌫ / ⇧⌘⌫ (a Mac's Delete key is Backspace); forward Delete still works |
   | Top / bottom of the playlist | Home / End | also ⌘↑ / ⌘↓ |

   Shape in code: a "primary modifier" (⌘ on macOS, Ctrl elsewhere) used by
   the key handler (`app.rs`, around line 1100) and by click modifiers,
   plus a macOS table for the rows above, and shortcut labels in
   `bind.rs`/`model.rs` taken from the same table so menus and keys agree.
   Linux and Windows keys and labels do not change. ⌘V reads the
   clipboard (NSPasteboard) and adds what it holds as `Remote::Add` items:
   URLs, or file paths. Check that winit's default app menu (Quit ⌘Q) ends
   through `App::exiting`, so the session is saved, and that its ⌘H (Hide)
   does not clash.
3. **Dialogs: `dialogs/osascript.rs`.** Each `Request` becomes an AppleScript
   run with `osascript`, off the UI thread like `zenity`:
   `choose file` (multiple, by type), `choose folder`, `choose file name`,
   `display dialog … default answer` (Entry, used by Open URL),
   `choose from list` (List), `display alert` (Info), a two-button
   `display dialog` (Question). Values go in as `on run argv` arguments,
   never spliced into the script text. The builder checks the dialogs come
   to the front over Popamp's windows.
4. **Fonts: `font/coretext.rs`.** `family(name)` resolves a family (generic
   names such as `sans-serif` mapped to the system's) to a file through
   CoreText's font URL attribute. `for_char(c)` uses CoreText's fallback for
   a string holding `c`. Many macOS fonts are `.ttc` collections or variable
   fonts: the interface may need a face index next to the path (fontdue's
   `FontSettings::collection_index`). That is a small change to the shared
   `font.rs`, kept in this PR.
5. **Paths.** `Paths::system()` on macOS: config and data both in
   `~/Library/Application Support/popamp` (one folder, as on Windows).
   `share_dirs` is the app bundle's `Contents/Resources` when running from
   a bundle. The portable `popamp.ini`-beside-the-program mode stays.
6. **Library names.** `libopus.0.dylib`, `libopenmpt.0.dylib`, looked up in
   the bundle's `Contents/Frameworks`, then by name, then Homebrew
   (`/opt/homebrew/lib`, `/usr/local/lib`), since `dlopen` does not search
   Homebrew by default. This matters for radio: Ogg Opus streams need
   libopus.
7. **Small swaps.** The plug-in journal opens with `open -t` instead of
   `xdg-open`.
8. **Opening files from Finder.** Finder and "Open With" send files as an
   Apple Event (`kAEOpenDocuments`), not on the command line, and winit 0.30
   does not pass these on. Install a handler with `NSAppleEventManager`
   after the event loop is built and before `run_app` (late handlers miss
   the event that launched the app). It forwards the paths as
   `AppEvent::Remote` open requests, as the IPC server does. The builder
   checks that AppKit or winit does not replace a handler installed before
   `run_app`; if it does, install it from `applicationWillFinishLaunching`
   or the earliest point that keeps it. Also a
   **minimal development bundle**: a script that wraps the built binary in
   `Popamp.app` with an `Info.plist` declaring the document types (audio,
   playlists including `.pls`, `.wsz`/`.psz`), because macOS only offers
   apps with a bundle for "Open With".

9. **Menus and tooltips that work.** *My call:* the skin-drawn pop-up
   menus are separate windows, and if macOS lets them take the focus
   (closing them, or stealing the keyboard), daily use is blocked. So this
   part of Stage 5's "window behaviour" moves here: menus and tooltips open
   without taking the focus and close as they do on Linux.

After stage 3 the contributor can use Popamp daily: classic rectangular
skins, radio from `.pls` in Finder and from Open URL.

### Stage 4: something people can download

- `packaging/build-macos.sh`, next to the Linux and Windows scripts:
  `Popamp.app` with `Info.plist` (bundle identifier, `LSMinimumSystemVersion`
  12.0, `NSHighResolutionCapable`, `CFBundleDocumentTypes`, exported UTIs
  for `.wsz` and `.psz`), an `.icns` made from the logo, libopus,
  libopenmpt and `GeneralUser-GS.sf2` inside, library install names fixed
  to `@executable_path/../Frameworks`, the bundle re-signed (ad hoc at
  least: Apple Silicon refuses unsigned code), and a `.dmg` through
  `hdiutil`. Both editions, `popamp` and `popamp-plus`, as on the other
  systems.
- The MIDI SoundFont search also looks in `Contents/Resources`.
- A macOS job in `release.yml`.
- Docs: README badge and install table, `docs/building.md` (requirements
  for macOS, Metal for visualizers), `CONTRIBUTING.md`, the
  "Linux and Windows" wording in module docs and `Cargo.toml`.
- Signing and notarisation depend on the maintainers (see open questions).
  Until then releases are ad-hoc signed and the docs say how to open them
  past Gatekeeper.
- Intel and universal builds can join here.

### Stage 5: everything else

- **Shaped skins** (`region.txt`): moved into Stage 3 by the spike after
  Stage 2 (see its result).
- **Media keys and Now Playing**: `media/nowplaying.rs` with
  `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter`, fed by the same
  `Snapshot`s, commands sent back the same way. Main-thread work is
  dispatched to the main queue.
- **"Open with Popamp" in Preferences**: `NSWorkspace`
  `setDefaultApplicationAtURL:toOpenContentType:` (macOS 12+) per UTI of
  each group; `defaults()` reads the current handler.
- **Keep awake** for visualizers: an `IOPMAssertion`, or a `caffeinate -d -w
  <pid>` child ended on drop.
- **Work areas and the bar skin**: `work_areas()` from `NSScreen`'s visible
  frames, converted from bottom-left points to Popamp's top-left pixels.
  `reserve()` cannot reserve; the bar floats at a high window level.
- **Window behaviour**: menus and tooltips must not steal focus, docked
  windows hide with a minimized main window (`OWNED_FOLLOW_OWNER` stays
  `false`, so Popamp moves docked windows itself), the Dock icon brings
  minimized windows back.
- **Visualizers** (`--features plus`) on Metal: verify; any change belongs in
  `popamp/popamp-visual`, a separate repository.

### Testing

- CI on macOS covers what runs without a screen and sound card: unit tests,
  `/RENDER` golden images, and the engine with the `null` device.
- The contributor tests by hand on their Apple Silicon Mac for each PR
  (sound, radio, dialogs, fonts, Finder opening, Retina size), recording
  what was checked in `docs/testing.md` as the project does.
- UI-visible PRs include `/RENDER` images (CONTRIBUTING.md).

## The maintainers' answer (popamp/popamp#32, 2026-11-12)

- macOS is welcome as a **community-maintained, experimental** platform:
  the contributor keeps it building and answers macOS bug reports; a
  failing macOS job does not block Linux and Windows releases, and a
  release may ship without macOS. The label goes after a couple of stable
  releases.
- The build-script cfg `linux_like` is approved.
- `~/Library/Application Support/popamp` is approved; `/INIDIR` keeps
  overriding it.
- The keymap and ⌘V paste are agreed; platform differences stay in one
  table in the code that menus and docs follow (`mac_keys.rs`).
- Signing is decided at Stage 4 upstream; they would rather not publish
  under another company's identity without thinking it through. Release
  builds must come from the repository's CI, from the tag.
- CI: GitHub's hosted macOS runners after merges to `main`; no
  self-hosted runners on a public repository.
- **Timing:** wait for their 0.9 release (it adds tray, global hotkeys and
  media-control platform modules the cfg split touches), then base Stage 1
  on `main` and send one stage at a time, small PRs, no behaviour change
  on Linux or Windows.

## Open questions for the maintainers (in the Stage 0 issue)

1. Do they want macOS as a supported platform, and with what expectation of
   who keeps it working?
2. The cfg split: an alias via `cfg_aliases`, or spelled-out cfgs? Alias
   name? (Stage 1 uses a build-script cfg, `linux_like`.)
3. `~/Library/Application Support/popamp` for settings and skins.
4. The Mac keymap (Stage 3, item 2) and ⌘V paste: macOS follows Mac
   conventions rather than Winamp's Windows keys.

## Decided by the contributor, offered in the issue

- **Bundle identifier:** `app.popamp.popamp`.
- **Signing and notarisation:** under the Apple Developer account of the
  contributor's company, Nimasystems, free of charge, until a non-profit
  organisation for Popamp exists with its own account.
- **CI:** the macOS job runs only after a merge to `main` (push), not on
  every pull request. GitHub's hosted macOS runners are free for public
  repositories and are the first choice; otherwise the contributor's own
  build servers run the checks. No server or network details go in the
  repository or its documents.

## Left to the builder

- Module and file names, the cfg alias name, ring-buffer implementation.
- Exact CoreAudio property calls and error messages.
- AppleScript wording and the type filters for `choose file`.
- How the font interface carries a collection face index.
- The macOS default zoom value, tuned on a real screen.
- How tests that need ALSA devices are adapted.
- PR boundaries inside Stage 3, as long as each PR stands alone and
  changes nothing on Linux or Windows.
