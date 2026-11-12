# macOS Stage 5b: The Rest

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill in what's left of the macOS placeholders:
- clicking the Dock icon brings Popamp back;
- visualizers keep the screen awake;
- the bar skin respects the menu bar and Dock;
- Preferences can make Popamp the default player for audio, playlists and skins;
- the release ships one universal disk image (Apple Silicon and Intel).

**Architecture:** Each item fills its stage-1 macOS placeholder behind the existing interface:
- the Dock icon: a second Apple Event (`rapp`) in `finder.rs`'s receiver;
- keep-awake: an IOKit power assertion in `keep_awake.rs`'s macOS `os`;
- work areas: `NSScreen` visible frames in `platform/macos.rs`;
- default apps: a macOS `Desktop` in `filetypes.rs`, behind the checkbox UI Linux already uses in `panels.rs`;
- universal builds: `ARCH=universal` in the packaging scripts and the release job.

**Tech Stack:** Rust 1.96+, objc2 0.6 / objc2-app-kit / objc2-foundation 0.3.2, `objc2-uniform-type-identifiers` 0.3.2 (`UTType`), IOKit's `IOPMAssertionCreateWithName` through `extern "C"`, `lipo`, the `x86_64-apple-darwin` Rust target.

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md`, "Stage 5, as approved on 2026-11-12 (after 0.9)", 5b.

## Global Constraints

- No behaviour change on Linux or Windows; macOS-only dependencies, `default-features = false`.
- fmt, `clippy -D warnings`, tests on the three feature sets; `Cargo.lock` committed.
- **No Claude notices** in commits, PR bodies or comments.
- **Tests must never change the machine's real settings.** No test calls NSWorkspace's default-app setters. Default apps are checked by hand.
- The contributor is away. Work on branch `macos/5b-rest` from `macos/5a-control`; push it and open a draft PR in the fork. Nothing goes upstream, and nothing is merged into the fork's `main`. Manual checks are listed for the contributor, not skipped silently.

## Review Focus

1. **Clicking the Dock icon with everything minimized, or with the main window hidden by "minimize to tray".** Expected: the windows come back. Manual (Task 6 list).
2. **Changing default apps when the running Popamp isn't a bundle** (`cargo run`). Expected: the checkboxes show and setting them says why it can't, rather than registering a bare binary. Pinned by `defaults_need_an_app_bundle` (Task 4).
3. **A visualizer in full screen for over the display's sleep time.** Expected: the screen stays on, and sleeps normally once the visualizer closes. Pinned by `a_hold_keeps_the_screen_on_until_dropped` (Task 2) and the manual list.
4. **Two screens, the bar skin on the secondary one.** Expected: the bar sits above that screen's Dock and below the menu bar where there is one. Pinned by `work_areas_count_from_the_top_left` (Task 3).
5. **The universal disk image on an Intel Mac.** Expected: it opens, and every binary inside has both architectures. Pinned by the self-check (`lipo -archs` = `arm64 x86_64`) in Task 5.

---

### Task 0: Branch

- [ ] `git switch -c macos/5b-rest macos/5a-control` (after 5a's review fixes).

### Task 1: The Dock icon brings Popamp back

**Files:** `apps/popamp/src/finder.rs`.

- Install a handler for `kCoreEventClass`/`kAEReopenApplication` (`'aevt'`/`'rapp'`) beside the open-documents one, at `WillFinishLaunching`.
- It sends `Remote::Show`, which un-minimizes and raises, the hidden companions included.
- Pure test: `const REOPEN_APPLICATION` is `u32::from_be_bytes(*b"rapp")` and differs from `OPEN_DOCUMENTS`. A trivial test is fine here; the behaviour is checked by hand.
- Commit: `Bring Popamp back when its Dock icon is clicked`.

### Task 2: Keep the screen on for visualizers

**Files:** `apps/popamp/src/keep_awake.rs` (the macOS `os` module; `cfg(feature = "visualizers")` as now).

- `IOPMAssertionCreateWithName(kIOPMAssertionTypePreventUserIdleDisplaySleep, kIOPMAssertionLevelOn /* 255 */, reason, &mut id)` with a `CFString` reason "Popamp visualizer". `Hold { id: u32 }`; `Drop` calls `IOPMAssertionRelease(id)`. Link `IOKit`.
- Test (macOS, with `--features popamp/visualizers`): `a_hold_keeps_the_screen_on_until_dropped`. `hold()` is `Some`, `pmset -g assertions` lists "Popamp visualizer" while it is held, and no longer lists it after drop.
- Commit: `Keep the Mac's screen on while a visualizer is in full screen`.

### Task 3: Work areas from the Mac's screens

**Files:** `apps/popamp/src/platform/macos.rs` (`work_areas`).

- Each `NSScreen::screens(mtm)`'s `visibleFrame`, which excludes the menu bar and Dock, in points with a bottom-left origin. Flip it to the top-left origin winit and Popamp use, with the first screen's `frame` height: `y = primary_h - (vy + vh)`.
- Pure: `fn flip(visible: (f64, f64, f64, f64), primary_h: f64) -> Screen`.
- Test `work_areas_count_from_the_top_left`: a 1512×982 primary with a 24-point menu bar and a 70-point Dock at the bottom gives `Screen { x: 0, y: 24, w: 1512, h: 888 }`. A screen above the primary (negative y in Popamp's units) flips correctly.
- `reserve` stays a no-op, since macOS reserves no strips. Its doc says the bar keeps out of the Dock and menu bar through the work area.
- Commit: `Keep the bar skin clear of the menu bar and Dock`.

### Task 4: Open with Popamp, in Preferences

**Files:**
- `apps/popamp/src/filetypes.rs`: a macOS `Desktop`, and `Group::label`/`extensions` for macOS;
- `apps/popamp/src/panels.rs`: the checkbox UI gated on `not(windows)` instead of `linux_like`; the macOS "Choose Popamp in Finder" label goes;
- `apps/popamp/src/widgets.rs`: `Form::checked` gated on `not(windows)`;
- `apps/popamp/Cargo.toml`: `objc2-uniform-type-identifiers` with `UTType`, and objc2-app-kit's `NSWorkspace`.

**Interfaces (macOS `Desktop`, the same as Linux's):** `from_env() -> Desktop`, `defaults(&self) -> [bool; 3]` and `set(&self, chosen: [bool; 3]) -> Result<(), String>`.
- `Desktop { app: Option<PathBuf> }`: the bundle the program runs in, if any (`Contents/MacOS` inside `….app`).
- `defaults`: a group counts as Popamp's when `NSWorkspace::URLForApplicationToOpenContentType` names this bundle for every type of the group (`UTType::typeWithFilenameExtension` of each of `Group::extensions`).
- `set`: for each newly ticked group, `setDefaultApplicationAtURL_toOpenContentType_completionHandler` for each type, with errors logged in the handler. Unticking leaves the user's other choices alone; macOS has no "unset". It returns `Err("Popamp is not running as Popamp.app")` without a bundle.
- Pure tests:
  - `defaults_need_an_app_bundle`: a `Desktop { app: None }` answers `Err` to `set` and `[false; 3]` to `defaults`, without touching NSWorkspace;
  - `each_group_has_types`: every group's extensions turn into a type.
- The existing Linux tests keep their gates. On macOS, the Preferences test `preferences_say_where_a_mac_chooses_the_player` is replaced by one asserting the three checkboxes are there.
- Commit: `Let Preferences make Popamp the Mac's default player`.

### Task 5: Universal builds

**Files:** `packaging/macos/fetch-libs.sh`, `packaging/build-macos.sh`, `.github/workflows/release.yml`, `docs/building.md`, `README.md`.

- **`fetch-libs.sh`** with `ARCH=universal` builds each library for `arm64` and `x86_64` (`--host=x86_64-apple-darwin`), then joins them with `lipo -create`.
- **`build-macos.sh`:**
  - `ARCH=universal` expects `lipo -archs` to be `arm64 x86_64` for every binary;
  - it names the disk image `…-macos-universal.dmg`;
  - its doc says so.
- **Release job:**
  - `rustup target add x86_64-apple-darwin`;
  - build each edition for both targets and join them with `lipo -create` into `target/popamp-player` and `target/popamp-plus`;
  - `ARCH=universal` for `fetch-libs.sh` and `build-macos.sh`;
  - artifact `macos-universal`.
- **Docs:** the README row becomes "macOS 12 or newer (Apple Silicon and Intel)", and `docs/building.md` names the universal image and how to build it.
- **Local check:** `rustup target add x86_64-apple-darwin`, build both, `lipo`, run both scripts with `ARCH=universal`. The self-check passes and `file` shows both architectures.
- Commit: `Build one disk image for Apple Silicon and Intel Macs`.

### Task 6: Checks, the draft PR, review, and the contributor's list

- [ ] Run all checks on the three feature sets, then the dev app (`build-dev-app.sh`).
- [ ] Push and open a draft PR in the fork: `--base macos/5a-control --head macos/5b-rest`, with no Claude notice.
- [ ] Final whole-branch review by a fresh reviewer, then the fix pass.
- [ ] Write the manual checks for the contributor into the ledger and the final message:
  - the Dock icon restores Popamp;
  - default apps from Preferences (a `.pls` then opens in Popamp from Finder);
  - a visualizer keeps the screen awake (Popamp Plus);
  - the bar skin on each screen;
  - the universal disk image, on an Intel Mac if one is at hand.
