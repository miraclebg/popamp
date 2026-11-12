# macOS Stage 0–1: The Upstream Issue, and Popamp Builds on macOS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draft the upstream issue proposing macOS support, and make the whole workspace build, lint and test on Apple Silicon macOS with stubs where Mac features come later, checked by a macOS CI job, with nothing changed on Linux or Windows.

**Architecture:** A build-script cfg, `linux_like` (every Unix but macOS), replaces `cfg(unix)` wherever the code means X11, ALSA, D-Bus, fontconfig, zenity, XDG or `mimeapps.list`. Plain `unix` stays only for POSIX code (Unix sockets, `sh` pipes, symlinks). Each OS-dependent area gets a macOS module behind its existing interface. In this stage those modules are stubs, named for what they will become (`coreaudio.rs`, `macos.rs`, `osascript.rs`, `coretext.rs`, `nowplaying.rs`).

**Tech Stack:** Rust 1.96+, winit 0.30, softbuffer 0.4, libloading, GitHub Actions `macos-15` (Apple Silicon).

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md` (branch `macos-port-design`). This plan covers its "Stage 0" and "Stage 1".

## Global Constraints

- macOS 12 or newer, Apple Silicon (`aarch64-apple-darwin`) first.
- No behaviour change on Linux or Windows; no new build-time system requirement there.
- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked`, each with default features, `--features popamp/plugins` and `--features popamp/plus`; `cargo check` with Rust 1.96.
- Dependencies MIT OR Apache-2.0 compatible. This stage adds none.
- One logical change per commit, with a message that says why, in the project's style (plain imperative sentence, no `feat:` prefix).
- House style: plain-English doc comments saying what a thing is for; one module per system behind one interface.
- Work happens on branch `macos/1-build`, made from `upstream/main`, never from `macos-port-design` (the spec and plans must not go upstream).
- Nothing goes to `popamp/popamp` (issue or PR) without the contributor approving the text first.
- Golden images (`apps/popamp/tests/golden/`) are not re-blessed in this stage.

## Review Focus

1. **A Linux or Windows feature quietly switched off by a wrong cfg** (for example the IPC socket or the `sh` pipe sources moved to `linux_like`). Expected: everything works exactly as before. Pinned by the cfg audit in Task 3, Step 6, and the full Linux and Windows CI runs in Task 4.
2. **Pressing Play on a Mac before sound output exists.** Expected: the track fails with a message saying macOS has no sound yet, not a silent race through the whole playlist on a no-op device. Pinned by `without_sound_output_a_track_fails_instead_of_racing` (Task 2).
3. **Opening an Opus or tracker file on a Mac without the Homebrew libraries.** Expected: the error names every place it looked. Pinned by `a_missing_library_names_every_place_tried` (Task 2).
4. **Menus and the playlist in Stage 1.** Expected: text is readable because a system font is found, even before CoreText. Pinned by `a_mac_always_has_a_sans_serif_font` (Task 3).
5. **A second `popamp` command on a Mac** (`popamp /NEXT`) reaching the running instance. Expected: it works. macOS caps socket paths at 104 bytes and `$TMPDIR` is long. Pinned by `socket_paths_fit_a_macos_socket_address` (Task 3).

---

### Task 0: Workspace setup

No product code. Every system change here needs the contributor's OK at execution time.

**Files:** none in the repo.

- [ ] **Step 1: Install Rust (ask the contributor first)**

Run: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && source "$HOME/.cargo/env" && rustc --version`
Expected: `rustc 1.9x.x` (1.96 or newer). Also `xcode-select -p` prints a path; if not, ask the contributor to run `xcode-select --install`.

- [ ] **Step 2: Install the libraries the tests load**

Run: `brew install opus libopenmpt && ls /opt/homebrew/lib/libopus.0.dylib /opt/homebrew/lib/libopenmpt.0.dylib`
Expected: both paths listed.

- [ ] **Step 3: Turn on GitHub Actions in the fork** (forks start with Actions off)

Run: `gh api -X PUT repos/miraclebg/popamp/actions/permissions -F enabled=true -f allowed_actions=all && gh api repos/miraclebg/popamp/actions/permissions`
Expected: `"enabled":true`.

- [ ] **Step 4: Make the work branch from upstream**

Run: `git fetch upstream && git switch -c macos/1-build upstream/main`
Expected: `Switched to a new branch 'macos/1-build'`.

- [ ] **Step 5: Confirm today's failure** (the stage's starting "red")

Run: `cargo check -p popamp 2>&1 | grep -m1 -E "error\[E0432\]|could not find .x11."`
Expected: an error pointing at `apps/popamp/src/platform/x11.rs` (`winit::platform::x11` does not exist on macOS).

---

### Task 1: The upstream issue (Stage 0)

**Files:**
- Create (on branch `macos-port-design`, not `macos/1-build`): `docs/superpowers/upstream/2026-11-12-macos-issue.md`

- [ ] **Step 1: Draft the issue**

Title: `macOS support: a plan, and an offer to build it`. Body sections, in the project's plain voice, drawn from the spec:
1. Who and why: a macOS user who wants to use Popamp daily and contribute the port.
2. Approach: one macOS module per area behind the existing interfaces, the way `alsa.rs`/`wasapi.rs` and `x11.rs`/`windows.rs` sit today. Apple frameworks through the `objc2` crates winit already brings in. Helper programs where Linux uses them (`osascript` like `zenity`). No change to Linux or Windows code.
3. Baseline: macOS 12+, Apple Silicon first, Intel later.
4. Stages 1–5 as the spec lists them, one small PR each (several for Stage 3), with a macOS CI job from the first PR.
5. The spec's seven "Open questions for the maintainers", numbered.
6. Mac-first conventions, with the keymap table from the spec's Stage 3, item 2.

- [ ] **Step 2: Show the contributor the draft and wait for approval or edits**

- [ ] **Step 3: Commit the approved text**

```bash
git switch macos-port-design
git add docs/superpowers/upstream/2026-11-12-macos-issue.md
git commit -m "Draft the upstream issue proposing macOS support"
git push
git switch macos/1-build
```

- [ ] **Step 4: Post it (only after the contributor says "post it")**

The file holds only the issue body; the title goes on the command line.
Run (from the `macos-port-design` checkout): `gh issue create -R popamp/popamp --title "macOS support: a plan, and an offer to build it" --body-file docs/superpowers/upstream/2026-11-12-macos-issue.md`
Expected: an issue URL. Record it in the PR description of Task 5.

---

### Task 2: `popamp-audio` builds and tests on macOS

**Files:**
- Create: `crates/popamp-audio/build.rs`
- Create: `crates/popamp-audio/src/dylib.rs`
- Create: `crates/popamp-audio/src/output/coreaudio.rs`
- Modify: `crates/popamp-audio/src/lib.rs` (add `mod dylib;`)
- Modify: `crates/popamp-audio/src/output/mod.rs:1-15`
- Modify: `crates/popamp-audio/src/opus.rs:21-30`, `crates/popamp-audio/src/tracker.rs:38-46`
- Modify: `crates/popamp-audio/src/engine.rs` (tests module)

**Interfaces:**
- Produces: `cfg(linux_like)`, set by `build.rs` when `CARGO_CFG_TARGET_FAMILY` contains `unix` and `CARGO_CFG_TARGET_OS != "macos"`, and declared with `cargo::rustc-check-cfg=cfg(linux_like)` so `unexpected_cfgs` stays quiet. Task 3 copies this build script logic.
- Produces: `output::Pcm` on macOS = `coreaudio::CoreAudioPcm` with the same API as `AlsaPcm`/`WasapiPcm`: `open(device: &str, rate: u32, channels: u32, latency_ms: u32) -> Result<Self, String>`, `write(&mut self, &[f32]) -> Result<(), String>`, `delay(&self) -> u64`, `pause(&mut self, bool) -> bool`, `discard(&mut self)`, `drain(&mut self)`, and the public fields `rate: u32` and `channels: u32`. Also `available() -> Result<(), String>` and `devices() -> Vec<Device>`. Stage 2 replaces the body, not the interface.
- Produces: `dylib::load(name: &str) -> Result<libloading::Library, String>` and `dylib::macos_candidates(name: &str, exe: Option<&Path>) -> Vec<PathBuf>` (the latter `#[cfg(any(target_os = "macos", test))]`).

- [ ] **Step 1: Write the failing tests**

In `dylib.rs` (these run on every OS, since `macos_candidates` is pure):

```rust
#[test]
fn macos_looks_in_the_bundle_then_where_dyld_looks_then_homebrew() {
    let exe = Path::new("/Applications/Popamp.app/Contents/MacOS/popamp");
    assert_eq!(
        macos_candidates("libopus.0.dylib", Some(exe)),
        [
            PathBuf::from("/Applications/Popamp.app/Contents/Frameworks/libopus.0.dylib"),
            PathBuf::from("libopus.0.dylib"),
            PathBuf::from("/opt/homebrew/lib/libopus.0.dylib"),
            PathBuf::from("/usr/local/lib/libopus.0.dylib"),
        ]
    );
}

#[test]
fn without_the_program_path_there_is_no_bundle_to_look_in() {
    assert_eq!(macos_candidates("x.dylib", None)[0], PathBuf::from("x.dylib"));
    assert_eq!(macos_candidates("x.dylib", None).len(), 3);
}

#[cfg(target_os = "macos")]
#[test]
fn a_missing_library_names_every_place_tried() {
    let e = load("libpopamp-no-such.0.dylib").unwrap_err();
    let exe = std::env::current_exe().ok();
    for place in macos_candidates("libpopamp-no-such.0.dylib", exe.as_deref()) {
        assert!(e.contains(&*place.to_string_lossy()), "{e}");
    }
}
```

In `output/coreaudio.rs`, `#[cfg(test)]`:

```rust
#[test]
fn null_takes_samples_and_plays_nothing() {
    let mut pcm = CoreAudioPcm::open("null", 44_100, 2, 100).unwrap();
    assert_eq!(pcm.write(&[0.0; 512]), Ok(()));
    assert_eq!((pcm.delay(), pcm.rate, pcm.channels), (0, 44_100, 2));
}

#[test]
fn other_devices_say_macos_has_no_sound_yet() {
    let e = CoreAudioPcm::open(super::DEFAULT, 44_100, 2, 100).err().unwrap();
    assert!(e.contains("macOS"), "{e}");
    assert!(available().is_err() && devices().is_empty());
}
```

In `engine.rs` tests, next to `plays_what_an_added_source_says_to_play`:

```rust
#[cfg(target_os = "macos")]
#[test]
fn without_sound_output_a_track_fails_instead_of_racing() {
    let wav = silent_wav("no-output", 1000);
    let (tx, events) = crossbeam_channel::unbounded();
    let engine = Engine::spawn(crate::output::DEFAULT.into(), Arc::new(Sources::default()), move |e| {
        let _ = tx.send(e);
    });
    engine.send(EngineCmd::Open { id: 1, location: wav.to_string_lossy().into(), start_ms: 0 });
    let seen: Vec<EngineEvent> = std::iter::from_fn(|| events.recv_timeout(Duration::from_secs(2)).ok()).collect();
    let _ = std::fs::remove_file(wav);
    assert!(seen.iter().any(|e| matches!(e, EngineEvent::Error { id: 1, message } if message.contains("macOS"))), "{seen:?}");
    assert!(!seen.iter().any(|e| matches!(e, EngineEvent::Finished { .. })), "{seen:?}");
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p popamp-audio 2>&1 | tail -5`
Expected: compile errors (`alsa` selected on macOS, `dylib` and `coreaudio` missing).

- [ ] **Step 3: Add `build.rs` setting `linux_like`**

Read `CARGO_CFG_TARGET_FAMILY`/`CARGO_CFG_TARGET_OS` (not `cfg!`, so cross-builds are right). Always print `cargo::rustc-check-cfg=cfg(linux_like)`. Print `cargo::rustc-cfg=linux_like` for Unix that is not macOS. Doc comment: "Linux and the other Unix systems with ALSA, X11 and the freedesktop services: every Unix but macOS."

- [ ] **Step 4: Select the output per system in `output/mod.rs`**

`#[cfg(unix)]` → `#[cfg(linux_like)]` on the three ALSA lines; add the same three lines for `#[cfg(target_os = "macos")]` with `coreaudio` / `CoreAudioPcm`. Update the module doc to "ALSA on Linux, WASAPI on Windows, CoreAudio on macOS (sound comes in a later change; only `null` plays for now)".

- [ ] **Step 5: Write the `coreaudio.rs` stub**

`null` opens and takes samples at once (as `null` does on Windows). Every other name, and `available()`, return `Err` with the constant text `"Popamp cannot play sound on macOS yet"`. `devices()` is empty. `delay` 0; `pause` true; `discard`/`drain` do nothing.

- [ ] **Step 6: Write `dylib.rs`**

`macos_candidates`: `exe.parent().parent()/Frameworks/<name>` when `exe` has two parents, then `<name>`, then `/opt/homebrew/lib/<name>`, then `/usr/local/lib/<name>`. `load` on macOS tries each candidate with `libloading::Library::new`. Its error is `cannot load <name> (tried <p1>, <p2>, …): <last error>`. On other systems `load` tries `name` alone, and the error is `cannot load <name>: <error>`, the same as today's text.

- [ ] **Step 7: Use `dylib::load` in `opus.rs` and `tracker.rs`**

Library names: Windows `opus.dll` / `libopenmpt.dll` (unchanged); `#[cfg(target_os = "macos")]` `libopus.0.dylib` / `libopenmpt.0.dylib`; `#[cfg(linux_like)]` `libopus.so.0` / `libopenmpt.so.0` (unchanged). Keep each caller's existing prefix (`tracker modules need libopenmpt: …`).

- [ ] **Step 8: Gate the ALSA-only engine tests**

In `engine.rs` tests, `worker_playing`, `a_new_device_goes_on_from_where_the_track_was`, `a_stream_goes_on_counting_on_the_new_device` and `a_paused_track_resumes_on_the_new_device_where_it_was` (they use ALSA's `plug:null`) go from `#[cfg(unix)]` to `#[cfg(linux_like)]`. Change that comment to say they need ALSA. `ShellSource`, `shows`, `plays_what_an_added_source_says_to_play` and `seeks_while_paused_show_the_last_one` stay `#[cfg(unix)]`.

- [ ] **Step 9: Run the crate's tests and lints on macOS**

Run: `cargo test -p popamp-audio && cargo clippy -p popamp-audio --all-targets -- -D warnings`
Expected: all pass, no warnings. If a `#[cfg(unix)]` engine test that uses `null` fails on macOS only because the stub's `null` does not pace like ALSA's, gate that test to `linux_like` too, with a comment saying why. Do not change the stub to make it pass.

- [ ] **Step 10: Commit**

```bash
git add crates/popamp-audio
git commit -m "Build popamp-audio on macOS, without sound yet

ALSA is only for Linux-like systems now. macOS gets an output that only
offers the null device, so a track fails with a message instead of
racing through the playlist. Opus and libopenmpt load from the app
bundle or Homebrew, which dyld does not search."
```

---

### Task 3: The `popamp` app builds and runs on macOS

**Files:**
- Modify: `apps/popamp/build.rs`, `apps/popamp/Cargo.toml` (target dependency sections)
- Create: `apps/popamp/src/platform/macos.rs`, `apps/popamp/src/dialogs/osascript.rs`, `apps/popamp/src/font/coretext.rs`, `apps/popamp/src/media/nowplaying.rs`
- Modify: `apps/popamp/src/platform/mod.rs:1-13`, `media/mod.rs:17-24`, `dialogs/mod.rs:1-14`, `font.rs:1-17`, `filetypes.rs` (cfgs at lines 9, 17, 21, 31, 83, 86), `widgets.rs:196`, `keep_awake.rs:34-66`, `panels.rs` (cfgs at lines 8, 42, 45, 454–475, 502–520, 547, 563, 598, tests at 1084–1130), `ipc.rs` (tests)

**Interfaces:**
- Consumes: the `linux_like` build-script logic from Task 2, Step 3 (copied into `apps/popamp/build.rs` next to the existing `#[cfg(windows)]` block, which keeps working).
- Produces (macOS, same signatures as `x11.rs`): `platform::attributes(role: WindowRole) -> WindowAttributes`, `platform::OWNED_FOLLOW_OWNER: bool = false`, `platform::Platform` with `connect() -> Platform`, `set_shape(&self, &Window, Option<&Mask>, (i32, i32)) -> Result<(), String>`, `reserve(&self, &Window, Strip) -> Result<(), String>`, `attach(&self, &Window, Option<&Window>) -> Result<(), String>`, `no_activate(&self, &Window)`, `release(&self, &Window)`, `work_areas(&self) -> Vec<Screen>`.
- Produces: `dialogs::osascript::execute(&Request) -> Vec<String>`; `font::coretext::{family(&str) -> Option<PathBuf>, for_char(char) -> Vec<PathBuf>}`; `media::nowplaying::spawn(EventLoopProxy<AppEvent>, Arc<Shared>, &str) -> Option<crossbeam_channel::Sender<Snapshot>>`; `keep_awake::os::{Hold, hold() -> Option<Hold>}` for macOS. Later stages fill these in.

- [ ] **Step 1: Write the failing tests**

`platform/macos.rs`:

```rust
#[test]
fn only_dialogs_have_a_title_bar() {
    for role in [WindowRole::Main, WindowRole::Docked, WindowRole::Bar, WindowRole::Popup, WindowRole::Tooltip] {
        assert!(!attributes(role).decorations, "{role:?}");
    }
    assert!(attributes(WindowRole::Dialog).decorations);
}

#[test]
fn menus_and_tooltips_float_without_taking_the_focus() {
    for role in [WindowRole::Popup, WindowRole::Tooltip] {
        let a = attributes(role);
        assert!(!a.active && a.window_level == WindowLevel::AlwaysOnTop, "{role:?}");
    }
}
```

`font/coretext.rs`:

```rust
#[test]
fn a_mac_always_has_a_sans_serif_font() {
    assert!(family("sans-serif").is_some_and(|p| p.is_file()));
}
```

`ipc.rs` tests:

```rust
#[cfg(target_os = "macos")]
#[test]
fn socket_paths_fit_a_macos_socket_address() {
    for class in ["default".to_string(), format!("new-{}", u32::MAX)] {
        let path = socket_path(&class);
        assert!(path.as_os_str().len() < 104, "{}", path.display());
    }
}
```

`panels.rs` tests:

```rust
#[cfg(target_os = "macos")]
#[test]
fn preferences_say_where_a_mac_chooses_the_player() {
    let m = songs(&[]);
    let p = Panel::Prefs(Preferences::new(m.settings(), vec![], test_types("mac")));
    let Panel::Prefs(pr) = &p else { unreachable!() };
    assert!(pr.form.labels().iter().any(|l| l.starts_with("Choose Popamp in Finder")));
}
```

(`Form::labels(&self) -> Vec<&str>`, `#[cfg(test)]` in `widgets.rs`, gives the texts of the `Widget::Label`s; add it if `Form` has no such accessor.)

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p popamp 2>&1 | grep -m3 error`
Expected: the `winit::platform::x11` error from Task 0.

- [ ] **Step 3: Cargo and build script**

`apps/popamp/Cargo.toml`: `[target.'cfg(unix)'.dependencies]` becomes `[target.'cfg(all(unix, not(target_os = "macos")))'.dependencies]`, because Cargo target tables cannot see build-script cfgs. `apps/popamp/build.rs`: add the `linux_like` logic from Task 2 and update the file's doc comment.

- [ ] **Step 4: Move every Linux-meaning `cfg(unix)` to `linux_like`, and add the macOS modules**

- `platform/mod.rs`: `x11` → `linux_like`; add `#[cfg(target_os = "macos")] mod macos; pub use macos::{attributes, Platform, OWNED_FOLLOW_OWNER};`. Module doc: "X11 (or Xwayland) on Linux, Win32 on Windows, AppKit on macOS". `shape_rects` gets `#[cfg_attr(target_os = "macos", allow(dead_code))]`.
- `platform/macos.rs`: base attributes `Window::default_attributes().with_decorations(false).with_resizable(false)`. `Popup` and `Tooltip` add `.with_window_level(WindowLevel::AlwaysOnTop).with_active(false)`. `Dialog` uses `.with_decorations(true)`. `Platform` is a unit struct. `set_shape`, `reserve` and `attach` return `Ok(())`. `work_areas` returns an empty `Vec` (the bar code already handles none, as when X11 has no connection). `set_shape` logs once at `info` that skin shapes are not drawn on macOS yet, through a `std::sync::Once`.
- `media/mod.rs`: `mpris` → `linux_like`; add `#[cfg(target_os = "macos")] mod nowplaying; use nowplaying as service;`. Doc: "… Now Playing on macOS (later)". `nowplaying::spawn` returns `None`.
- `dialogs/mod.rs`: `zenity` → `linux_like`; add `osascript` for macOS. `osascript::execute` logs once at `warn` ("dialogs are not there on macOS yet") and returns an empty `Vec`, so a Question counts as "no".
- `font.rs`: `fontconfig` → `linux_like`; add `coretext` for macOS. `coretext::family` returns, whatever the name, the first existing of `/System/Library/Fonts/Geneva.ttf` and `/System/Library/Fonts/Helvetica.ttc`. `for_char` returns an empty `Vec`. Doc says CoreText comes later.
- `filetypes.rs`: `any(unix, test)` → `any(linux_like, test)` (lines 9, 17); `unix` → `linux_like` (21, 31, 83, 86).
- `widgets.rs:196` (`Form::checked`): `unix` → `linux_like`.
- `keep_awake.rs`: the D-Bus `os` module → `linux_like`. A macOS `os` module has `pub struct Hold;` and `pub fn hold() -> Option<Hold>`, which logs `warn` ("cannot keep the screen on: not on macOS yet") and returns `None`.
- `panels.rs`: every `#[cfg(unix)]` (lines 8, 42, 472, 502, 547, 598 and the tests at 1084–1126) → `linux_like`. `#[cfg(windows)] struct FileTypes` and its `impl` → `#[cfg(not(linux_like))]`, and its doc becomes "Windows and macOS keep the choice to themselves". `#[cfg_attr(windows, allow(dead_code))]` → `#[cfg_attr(not(linux_like), allow(dead_code))]`. The test helper `test_types` for Windows → `not(linux_like)`. In `Preferences::new`, next to the Windows block, add for `target_os = "macos"` a label at `Rect::new(22, y + 18, w - 44, 24)` with text exactly `Choose Popamp in Finder: Get Info, then Open with.`
- `ipc.rs` and everything else listed under Step 6 stays `unix`.

- [ ] **Step 5: Run the tests and lints, all three feature sets**

Run: `for f in "" "--features popamp/plugins" "--features popamp/plus"; do cargo clippy --workspace --all-targets --locked $f -- -D warnings && cargo test --workspace --locked $f || break; done`
Expected: clean and passing. Exceptions:
- `--features popamp/plus` failing to build inside the `popamp-visual` crate: stop and report it to the contributor. A fix belongs in `popamp/popamp-visual`, a separate repository.
- Golden images differing on macOS: stop and report, do not re-bless.
- Any other failure: fix it with the smallest macOS-only change, or gate a test that relies on Linux-only behaviour to `linux_like` with a comment saying why.

- [ ] **Step 6: Audit what stays `unix`**

Run: `grep -rnE 'cfg\((any\()?unix|cfg!\(unix|cfg_attr\(unix' apps crates --include=*.rs`
Expected: hits only in `apps/popamp/src/ipc.rs` (the Unix-socket transport), `crates/popamp-audio/src/{pipe,engine,tags}.rs` (`sh` pipes, POSIX file semantics), `crates/popamp-core/src/plformats.rs` (Unix paths) and `crates/popamp-plugins/src/{files,host}.rs` (symlinks, `sh`). Each must be true POSIX behaviour. Anything else is a mistake from Step 4.

- [ ] **Step 7: Run it by hand on the Mac**

Run: `cargo run -p popamp -- /NEW /INIDIR=/tmp/popamp-dev` and, from a second terminal, `cargo run -p popamp -- /NEXT /CLASS=default` (or `/STATUS`).
Expected: the main window, equalizer and playlist appear with the default skin, and menus show text. The log says sound output does not work on macOS yet. The second command reaches the first instance. Note anything else odd for the PR description, without fixing it here.

- [ ] **Step 8: Commit**

```bash
git add apps/popamp
git commit -m "Build and run the player on macOS, with stubs for now

cfg(unix) meant Linux, so macOS got X11, D-Bus, fontconfig and zenity
and the build failed. Those are for Linux-like systems now. macOS gets
its own module in each place, doing nothing yet or the least that
works, so later changes can fill them in one at a time."
```

---

### Task 4: macOS in CI

**Files:**
- Modify: `.github/workflows/ci.yml` (new job after `windows`)

- [ ] **Step 1: Add the job**

```yaml
  macos:
    name: macOS - clippy, test
    runs-on: macos-15
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      # Loaded at run time by the Opus and tracker module decoders.
      - run: brew install opus libopenmpt
```

followed by the same six `cargo clippy` / `cargo test` lines as the `windows` job.

- [ ] **Step 2: Commit and push, and open a draft PR inside the fork so CI runs on all three systems**

```bash
git add .github/workflows/ci.yml
git commit -m "Check macOS in CI, as Linux and Windows are"
git push -u origin macos/1-build
gh pr create -R miraclebg/popamp --base main --head macos/1-build --draft \
  --title "macOS: build and test (stage 1)" --body "Fork-internal run of CI; not for upstream yet."
```

- [ ] **Step 3: Wait for CI**

Run: `gh pr checks -R miraclebg/popamp macos/1-build --watch`
Expected: `check`, `windows`, `macos` and `minimum Rust version` all pass. Linux and Windows passing unchanged is the proof for Review Focus 1.

---

### Task 5: The upstream pull request (only after the issue has an answer and the contributor approves)

**Files:** none.

- [ ] **Step 1: Rebase on upstream and re-run CI**

Run: `git fetch upstream && git rebase upstream/main && git push --force-with-lease`, then `gh pr checks -R miraclebg/popamp macos/1-build --watch`.
Expected: green.

- [ ] **Step 2: Draft the PR description and show it to the contributor**

Contents:
- what changes and why (the cfg split and the stubs);
- "no behaviour change on Linux or Windows", with the evidence: CI green on all three, and the Task 3 Step 6 audit list;
- what was tried by hand on the Mac (Task 3 Step 7);
- a link to the issue from Task 1;
- the maintainers' answers this PR follows, such as the cfg naming.

- [ ] **Step 3: Open it (only after the contributor says so)**

Run: `gh pr create -R popamp/popamp --base main --head miraclebg:macos/1-build --title "Build and test on macOS (no sound or Mac features yet)" --body-file <approved file>`
Expected: a PR URL. Close the fork-internal draft PR from Task 4.
