# macOS Stage 3a: Daily Use

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Popamp usable every day on a Mac: right size on Retina screens, real dialogs (Open File/Folder/URL, Save, questions), system fonts through CoreText, Mac settings folders, and `.pls`/audio files opened from Finder through a development `Popamp.app`.

**Architecture:** Each item lives where its Linux and Windows counterparts live:
- the Retina factor in `platform` plus one multiplication in `bind::scale`;
- `dialogs/osascript.rs` running AppleScript the way `zenity.rs` runs zenity;
- `font/coretext.rs` behind the same two functions as `fontconfig.rs`;
- `Paths::system()` for macOS in `popamp-core`;
- a new macOS-only `finder.rs` that receives Finder's Apple Events and forwards them as the IPC server's open requests.

Linux and Windows code paths do not change.

**Tech Stack:** Rust 1.96+, winit 0.30, `objc2` 0.6, `objc2-foundation` 0.3.2, `objc2-core-text` 0.3.2, `objc2-core-foundation` 0.3.2 (the 0.6/0.3 family already in `Cargo.lock`), `osascript`, `plutil`/`codesign` for the development bundle.

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md` (branch `macos-port-design`): Stage 3, items 1, 3, 4, 5, 7, 8 and 9. Items 2 (keymap, ⌘V) and the shaped skins are **Stage 3b**, a separate plan; item 6 (library names) was done in Stage 1.

## Global Constraints

- macOS 12+, Apple Silicon first. No behaviour change on Linux or Windows. New dependencies are macOS-only, with `default-features = false` and only the features used.
- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked`, each with default features, `--features popamp/plugins` and `--features popamp/plus`; `Cargo.lock` committed.
- House style: plain-English doc comments; one module per system behind the existing interface; the UI thread does no I/O (dialogs already run off it).
- Bundle identifier `app.popamp.popamp`; `LSMinimumSystemVersion` 12.0.
- Settings and data on macOS: `~/Library/Application Support/popamp` (one folder, as on Windows). The portable `popamp.ini`-beside-the-program mode stays.
- Work on branch `macos/3-daily` from `macos/2-sound` in `/Users/mkovachev/sworkspace/popamp-macos`. Each task is one commit, and each could become its own upstream PR. Nothing goes upstream in this plan.
- Golden images are not re-blessed (none of this changes a `/RENDER` image: headless rendering keeps factor 100).

## Review Focus

1. **Moving the main window between the MacBook's Retina screen and the non-Retina LG monitor.** Expected: the windows take the size the zoom says on each screen, with no half or double size left behind. Pinned by the `ScaleFactorChanged` handling and the manual check in Task 6.
2. **Cancelling any dialog** (Esc, or the Cancel button). Expected: nothing happens; for the question "remove files from disk?", nothing is deleted. Pinned by `a_cancelled_dialog_gives_nothing` (Task 2).
3. **A file or URL name with quotes, backslashes or non-Latin letters** (`Мечта "live".mp3`, a URL with `&`). Expected: it opens, and no script breaks or runs. Pinned by `values_go_in_as_arguments_never_as_script` (Task 2).
4. **Double-clicking a `.pls` in Finder while Popamp is closed, and while it is already running.** Expected: it opens and plays both times; while running, there is no second Popamp. Pinned by the manual check in Task 5, Step 6.
5. **Playlist text in Cyrillic or Chinese** (tags in other scripts). Expected: drawn, not blank boxes, through CoreText's fallback. Pinned by `a_character_the_font_lacks_comes_from_a_font_that_has_it` (Task 3).

---

### Task 0: Branch

- [ ] **Step 1:** `cd /Users/mkovachev/sworkspace/popamp-macos && git switch macos/2-sound && git switch -c macos/3-daily`. Expected: `Switched to a new branch 'macos/3-daily'`.

---

### Task 1: Retina size

**Files:**
- Modify: `apps/popamp/src/platform/{mod.rs,macos.rs,x11.rs,windows.rs}` (a constant each)
- Modify: `apps/popamp/src/model.rs` (one field), `apps/popamp/src/bind.rs:239-245` (`scale`) and its tests
- Modify: `apps/popamp/src/app.rs` (set the factor when the main window is made; `ScaleFactorChanged` in `window_event`)

**Interfaces:**
- Produces: `platform::SKIN_COUNTS_IN_POINTS: bool`. It is `true` on macOS and `false` on X11 and Windows, where the zoom is in screen pixels and the desktop's scaling is the user's business.
- Produces: `Model::backing_pct: i32`, the main window's `scale_factor()` × 100, rounded; 100 until a window exists and in headless rendering.
- Changes: `bind::scale(model, key)` multiplies the zoom percent by `backing_pct / 100` when `SKIN_COUNTS_IN_POINTS`, before `Scale::new`. It applies to both `Scale::of` and the bar's `bar_zoom`.

- [ ] **Step 1: Write the failing tests** (in `bind.rs` tests)

```rust
#[cfg(target_os = "macos")]
#[test]
fn a_retina_screen_counts_the_skin_in_points() {
    let mut m = Model::for_tests();
    m.backing_pct = 200;
    assert_eq!(scale(&m, WindowKey::Main).px(275), 550, "x1 is 275 points: 550 pixels");
    m.settings_mut().dsize = true; // zoom 200
    assert_eq!(scale(&m, WindowKey::Main).px(275), 1100);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn elsewhere_the_zoom_is_in_screen_pixels() {
    let mut m = Model::for_tests();
    m.backing_pct = 200;
    assert_eq!(scale(&m, WindowKey::Main).px(275), 275);
}
```

- [ ] **Step 2:** Run `cargo test -p popamp bind::` and confirm it fails because `backing_pct` doesn't exist.
- [ ] **Step 3: Implement.** Add the field (default 100) and the constant, and multiply in `bind::scale`.
  - In `App`, after the main window is created in `create_window`, set `model.backing_pct` from `window.scale_factor()`.
  - On `WindowEvent::ScaleFactorChanged` for the main window, update `backing_pct` and run the same path `UiReq::Layout` takes (`apply_layout(WindowKey::Main)`), so every window is resized.
  - Menus and dialogs already scale through `dialog_pct()`; leave them.
- [ ] **Step 4:** Run `cargo test -p popamp` and `cargo clippy -p popamp --all-targets -- -D warnings`. Expected: pass.
- [ ] **Step 5: Look at it** (manual).
  - Run `cargo run -p popamp --release -- /NEW /INIDIR=/tmp/popamp-dev`.
  - Expected: the main window is 275 points wide, the size of Winamp on a non-Retina screen. ⌘D isn't there yet, so use the menu's Double Size: it doubles.
  - Ask the contributor whether 1× is a good default. If they want it larger, the default stays 1× and they turn on Double Size (the zoom setting already exists). Record the answer in the ledger.
- [ ] **Step 6: Commit:** `Draw skins at their size in points on Retina Macs`, with a body explaining that macOS counts the zoom in points, so 1× is 275 points, not 275 pixels.

---

### Task 2: Dialogs through `osascript`

**Files:**
- Modify: `apps/popamp/src/dialogs/osascript.rs` (replace the stub), `apps/popamp/src/dialogs/mod.rs` (drop the macOS `allow(dead_code)` on `Request`)

**Interfaces:**
- Keeps: `pub fn execute(req: &Request) -> Vec<String>`, with the same results as `zenity.rs`:
  - `Files`: the chosen paths;
  - `Folder` and `Save`: one path;
  - `Entry`: the typed text;
  - `List`: the chosen row's **index** as a string;
  - `Info`: empty;
  - `Question`: `["yes"]` or empty;
  - a cancelled dialog: empty.
- Produces (private, pure, for tests): `fn script(req: &Request) -> (&'static str, Vec<String>)`, the AppleScript (a fixed text per request kind) and its `argv`. Also `fn answer(req: &Request, stdout: &str) -> Vec<String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn values_go_in_as_arguments_never_as_script() {
    let req = Request::Entry { title: "Open URL".into(), text: "Address:".into(), initial: "http://a/?x=\"1\"&y=\\".into() };
    let (source, args) = script(&req);
    assert!(!source.contains("http://a"), "{source}");
    assert!(args.iter().any(|a| a == "http://a/?x=\"1\"&y=\\"), "{args:?}");
}

#[test]
fn file_filters_become_types() {
    let req = Request::Files { title: "Open".into(), multiple: true, filter: vec!["*.mp3".into(), "*.FLAC".into()], start: None };
    let (_, args) = script(&req);
    assert!(args.contains(&"mp3".to_string()) && args.contains(&"flac".to_string()), "{args:?}");
}

#[test]
fn a_list_answers_with_the_row_number() {
    let req = Request::List { title: "Presets".into(), column: "Name".into(), rows: vec!["Rock".into(), "Pop".into()] };
    assert_eq!(answer(&req, "Pop\n"), ["1"]);
}

#[test]
fn a_cancelled_dialog_gives_nothing() {
    for req in [
        Request::Question { title: "Remove".into(), text: "Delete from disk?".into() },
        Request::Files { title: "Open".into(), multiple: false, filter: vec![], start: None },
    ] {
        assert!(answer(&req, "").is_empty());
    }
}
```

- [ ] **Step 2:** Run `cargo test -p popamp dialogs::osascript` and confirm it fails (`script`/`answer` missing).
- [ ] **Step 3: Implement.** Run `osascript -e <script> <args…>` and read stdout. A non-zero exit, which is how AppleScript reports Cancel (error -128), gives empty.
  - Every script starts with `activate`, so the dialog comes to the front, and reads its values from `on run argv`.
  - `Files`: `choose file with prompt … of type {…} multiple selections allowed …`, with `default location` when `start` is set; print each item's `POSIX path` on its own line. An empty filter means no `of type`.
  - `Folder`: `choose folder`.
  - `Save`: `choose file name with prompt … default name … default location …`.
  - `Entry`: `display dialog … default answer …`, then `text returned`.
  - `List`: `choose from list`; map the chosen row's text back to its index. If rows repeat, take the first.
  - `Info`: `display dialog … buttons {"OK"} default button 1`.
  - `Question`: `display dialog … buttons {"No", "Yes"} default button "Yes" cancel button "No"`.
  - Remove the macOS `allow(dead_code)` on `Request`.
- [ ] **Step 4:** Run the tests and clippy. Expected: pass.
- [ ] **Step 5: Try it** (manual): Open file (several at once), Open folder, Open URL with SomaFM's direct link `https://ice1.somafm.com/groovesalad-128-mp3`, Save playlist, Load preset. Each appears in front, works, and cancels cleanly.
- [ ] **Step 6: Commit:** `Show Mac dialogs through osascript`.

---

### Task 3: System fonts through CoreText

**Files:**
- Modify: `apps/popamp/Cargo.toml` (macOS dependencies), `Cargo.lock`
- Modify: `apps/popamp/src/font/coretext.rs` (replace the stub)

**Interfaces:**
- Keeps: `pub fn family(name: &str) -> Option<PathBuf>` and `pub fn for_char(c: char) -> Vec<PathBuf>`.

- [ ] **Step 1: Add the dependencies** (macOS section): `objc2-core-text = { version = "0.3.2", default-features = false, features = ["std", "CTFont", "CTFontDescriptor"] }` and `objc2-core-foundation = { version = "0.3.2", default-features = false, features = ["std", "CFString", "CFURL", "CFCGTypes"] }`. Use the feature names as the crates spell them; check with `cargo info`.
- [ ] **Step 2: Write the failing tests** (replace `a_mac_always_has_a_sans_serif_font`):

```rust
/// Whether the font file at `path` draws `c`, as Popamp will read it.
fn draws(path: &std::path::Path, c: char) -> bool {
    let face = fontdue::Font::from_bytes(std::fs::read(path).unwrap(), fontdue::FontSettings::default()).unwrap();
    face.lookup_glyph_index(c) != 0
}

#[test]
fn generic_families_are_the_macs_own_fonts() {
    for name in ["sans-serif", "serif", "monospace"] {
        let path = family(name).unwrap_or_else(|| panic!("{name}"));
        assert!(draws(&path, 'A'), "{name}: {}", path.display());
    }
}

#[test]
fn a_character_the_font_lacks_comes_from_a_font_that_has_it() {
    for c in ['Ж', '中'] {
        assert!(for_char(c).iter().any(|p| draws(p, c)), "{c}: {:?}", for_char(c));
    }
}
```

- [ ] **Step 3:** Run `cargo test -p popamp font::coretext` and confirm the tests fail, because the stub returns Geneva and an empty fallback list.
- [ ] **Step 4: Implement.**
  - **`family`:** map the generic names to Mac families, `sans-serif` to `Helvetica Neue`, `serif` to `Times New Roman` and `monospace` to `Menlo`; other names pass through. Then `CTFontCreateWithName`, then `CTFontCopyAttribute(kCTFontURLAttribute)`, which gives a `CFURL` and then a path.
  - **`for_char`:** `CTFontCreateForString` with a Helvetica Neue base and the character's string; return its URL's path.
  - **Collections:** a `.ttc` file is read at face 0 by `font.rs`. Mac collections such as PingFang and Hiragino cover the same scripts in every face, so there is no face index for now (the spec's "may need" turned out unneeded). If the test above fails for that reason, stop and rule in the ledger.
- [ ] **Step 5:** Run the tests and clippy. Expected: pass.
- [ ] **Step 6: Commit:** `Find Mac system fonts and fallbacks through CoreText`.

---

### Task 4: Mac folders, and the plug-in journal

**Files:**
- Modify: `crates/popamp-core/src/config.rs:338-360` (`Paths::system`)
- Modify: `apps/popamp/src/plugins.rs:259`

**Interfaces:**
- Produces: `fn macos_paths(home: &Path, exe: Option<&Path>) -> Paths`, `#[cfg(any(target_os = "macos", test))]`.
  - `config_dir` and `data_dir` are both `home/Library/Application Support/popamp`.
  - `share_dirs` is `[<bundle>/Contents/Resources/popamp]` when `exe` sits in `….app/Contents/MacOS`, else empty.
- `Paths::system()`: the XDG version's `#[cfg(not(windows))]` becomes `#[cfg(all(not(windows), not(target_os = "macos")))]`; a `#[cfg(target_os = "macos")]` version calls `macos_paths`.

- [ ] **Step 1: Write the failing tests** (in `config.rs` tests)

```rust
#[test]
fn a_mac_keeps_settings_in_application_support() {
    let home = Path::new("/Users/u");
    let p = macos_paths(home, Some(Path::new("/Users/u/bin/popamp")));
    let support = PathBuf::from("/Users/u/Library/Application Support/popamp");
    assert_eq!((p.config_dir.clone(), p.data_dir.clone()), (support.clone(), support));
    assert!(p.share_dirs.is_empty());
}

#[test]
fn an_app_bundle_shares_its_resources() {
    let p = macos_paths(Path::new("/Users/u"), Some(Path::new("/Applications/Popamp.app/Contents/MacOS/popamp")));
    assert_eq!(p.share_dirs, [PathBuf::from("/Applications/Popamp.app/Contents/Resources/popamp")]);
}
```

- [ ] **Step 2:** Run `cargo test -p popamp-core config` and confirm it fails (`macos_paths` missing).
- [ ] **Step 3: Implement** as in Interfaces.
  - The bundle check is the same as `popamp-audio`'s `dylib::macos_candidates`: the program's folder is `MacOS` and its parent is `Contents`.
  - In `plugins.rs`, the journal viewer is `open -t <path>` on macOS (`Command::new("open").args(["-t"]).arg(&path)`), `notepad` on Windows, and `xdg-open` elsewhere.
- [ ] **Step 4:** Run the tests and clippy. Expected: pass.
- [ ] **Step 5: Commit:** `Keep Mac settings in Application Support; open the journal with open -t`.

---

### Task 5: Opening files from Finder

**Files:**
- Create: `apps/popamp/src/finder.rs` (macOS only; `mod finder;` under `#[cfg(target_os = "macos")]` in `main.rs`)
- Modify: `apps/popamp/src/main.rs` (call `finder::listen(proxy.clone())` after the event loop is built, before `run_app`)
- Modify: `apps/popamp/Cargo.toml` (macOS: `objc2 = "0.6"`, `objc2-foundation` 0.3.2 with the `NSAppleEventManager`, `NSAppleEventDescriptor`, `NSNotification`, `NSString`, `NSURL` features)
- Create: `packaging/macos/Info.plist`, `packaging/macos/build-dev-app.sh`

**Interfaces:**
- Produces: `pub fn listen(proxy: EventLoopProxy<AppEvent>)`.
  - It registers an observer for `NSApplicationWillFinishLaunchingNotification`.
  - When that fires, it installs an `NSAppleEventManager` handler for `kCoreEventClass`/`kAEOpenDocuments`. AppKit installs its own handlers during `finishLaunching`, so one installed earlier would be replaced; installing it here keeps it and still catches the event that launched the app.
  - The handler sends `AppEvent::Remote(Request { remotes: vec![Remote::Open(paths)], reply })`, as `media::send` does.
- Produces (pure, for tests): `fn file_paths(list: &NSAppleEventDescriptor) -> Vec<String>`, the `fileURLValue` paths of the event's direct-object list.

- [ ] **Step 1: Write the failing test** (in `finder.rs`)

```rust
#[test]
fn an_open_event_gives_the_paths_of_its_files() {
    let list = NSAppleEventDescriptor::listDescriptor();
    for (i, p) in ["/tmp/a b.pls", "/tmp/Мечта.mp3"].iter().enumerate() {
        let url = NSURL::fileURLWithPath(&NSString::from_str(p));
        list.insertDescriptor_atIndex(&NSAppleEventDescriptor::descriptorWithFileURL(&url), (i + 1) as isize);
    }
    assert_eq!(file_paths(&list), ["/tmp/a b.pls", "/tmp/Мечта.mp3"]);
}
```

- [ ] **Step 2:** Run `cargo test -p popamp finder` and confirm it fails (the module doesn't exist).
- [ ] **Step 3: Implement `finder.rs`.**
  - Define a class with `objc2::define_class!` that holds the proxy and has two methods: `willFinishLaunching:`, which installs the event handler, and `openDocuments:withReplyEvent:`.
  - `listen` creates one instance, keeps it alive for the program's lifetime, and adds it as the observer.
  - Doc comments explain why the handler goes in at `WillFinishLaunching`.
- [ ] **Step 4: The development bundle.**
  - `packaging/macos/Info.plist`:
    - `CFBundleIdentifier` `app.popamp.popamp`;
    - `CFBundleName`/`CFBundleExecutable` `popamp`;
    - `CFBundleShortVersionString` filled in by the script from `popamp /VERSION`;
    - `LSMinimumSystemVersion` 12.0, `NSHighResolutionCapable` true;
    - `CFBundleDocumentTypes` for audio (`LSItemContentTypes` `public.audio`), playlists (`CFBundleTypeExtensions` `m3u m3u8 pls xspf cue`) and skins (`wsz psz`), all with `CFBundleTypeRole` `Viewer`.
  - `packaging/macos/build-dev-app.sh [binary] [out]` (default `target/release/popamp` and `target/Popamp.app`):
    - makes `Contents/MacOS` and copies the binary;
    - writes `Info.plist`, then checks it with `plutil -lint`;
    - signs ad hoc with `codesign --force --sign -`.
  - The full release bundle (icon, libraries, DMG) is Stage 4.
- [ ] **Step 5:** Run the tests and clippy. Expected: pass. `sh packaging/macos/build-dev-app.sh` → `target/Popamp.app`; `plutil -lint` OK.
- [ ] **Step 6: Try it** (manual, with the contributor):
  1. With Popamp closed: `open -a target/Popamp.app ~/Music/<a song>` plays it.
  2. A downloaded `.pls` (`curl -o ~/Downloads/groovesalad.pls https://somafm.com/groovesalad.pls`), via "Open With → Popamp" in Finder, plays the station.
  3. With Popamp running, doing it again opens in the same instance.
  4. Optionally, set Popamp as the `.pls` default in Finder's Get Info and double-click the file.
- [ ] **Step 7: Commit:** `Open files Finder sends, through Apple Events; add a development Popamp.app`.

---

### Task 6: The whole workspace, and a day's use on the Mac

- [ ] **Step 1:** Run all checks on all three feature sets: `cargo fmt --all -- --check && for f in "" "--features popamp/plugins" "--features popamp/plus"; do cargo clippy --workspace --all-targets --locked $f -- -D warnings && cargo test --workspace --locked $f || break; done`. Expected: clean and passing.
- [ ] **Step 2: The menus and screens check** (with the contributor; spec Stage 3, item 9):
  - Right-click menus open where clicked and close when an item is chosen or when clicking elsewhere, and the main window keeps the keyboard.
  - Tooltips don't take the focus.
  - Moving the main window between the MacBook screen and the LG monitor resizes correctly (Review Focus 1).
  - If menus misbehave, stop and rule: the fix is part of this plan, as Stage 3, item 9 requires.
- [ ] **Step 3:** Push and open the stacked draft PR: `git push -u origin macos/3-daily && gh pr create -R miraclebg/popamp --base macos/2-sound --head macos/3-daily --draft --title "macOS: daily use (stage 3a)" --body "Fork-internal; stacked on #2."`. CI (Linux, Windows, MSRV) green.
