# macOS Stage 3b: Mac Feel

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Popamp on a Mac behaves like a Mac app: ⌘ is the main modifier with the agreed keymap and ⌘ labels in menus, ⌘V pastes links or files into the playlist, and classic skins with a `region.txt` shape are cut to their shape, with clicks passing through the cut-out parts.

**Architecture:**
- **Keys:** a pure macOS key table (`apps/popamp/src/mac_keys.rs`) turns a Mac combination into the Linux/Windows combination the existing handler already knows, or into a Mac-only action (paste, close window, minimize). The same table turns menu shortcut labels into Mac ones. The handler and the menus keep one source of truth.
- **Paste:** reads `NSPasteboard`: file URLs, else text lines that are web addresses or existing files. They are added like the control socket's `/ADD`.
- **Shapes:** a Core Animation mask layer, as the spike proved (`docs/superpowers/spikes/2026-11-12-macos-shapes.patch`). Click-through comes from polling the global cursor and switching each shaped window's hit-testing (`set_cursor_hittest`) by whether the cursor is over its mask.

Linux and Windows do not change.

**Tech Stack:** Rust 1.96+, winit 0.30, `objc2` 0.6, `objc2-app-kit` 0.3.2 (`NSPasteboard`), `objc2-core-graphics` 0.3.2 (`CGImage`, `CGEvent`), `objc2-quartz-core` 0.3.2 (`CALayer`), all of the 0.6/0.3 family in `Cargo.lock`.

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md` (branch `macos-port-design`): Stage 3, item 2 (keymap and ⌘V, as decided), and the shaped-skin spike's result (shapes move into Stage 3, with click-through).

## Global Constraints

- **Platforms:** macOS 12+, Apple Silicon first. No behaviour change on Linux or Windows; new dependencies are macOS-only, `default-features = false`.
- **Checks:** fmt, `clippy -D warnings` and tests on default, `popamp/plugins` and `popamp/plus` features; `Cargo.lock` committed.
- **The keymap, as decided with the contributor** (Mac first). ⌘ does what Ctrl does elsewhere, except:

  | Action | Linux/Windows | macOS |
  |---|---|---|
  | Quit | Alt+F4 | ⌘Q (winit's app menu) |
  | Preferences | Ctrl+P | ⌘, |
  | Close window / minimize | none | ⌘W (as its close button) / ⌘M |
  | Windowshade | Ctrl+W | ⇧⌘W |
  | Cycle Popamp's windows | Ctrl+Tab | ⌘` |
  | Always on top | Ctrl+A (main), Ctrl+Alt+A (playlist) | ⌥⌘A everywhere; ⌘A selects all in the playlist |
  | File info / invert selection | Alt+3 / Ctrl+I | ⌘I / ⇧⌘I |
  | Sort by title, file name, path | Ctrl+Shift+1/2/3 | ⌥⌘1/2/3 |
  | Stop after current | Ctrl+V | ⌥⌘V |
  | Paste links or files into the playlist | none | ⌘V |
  | Remove / crop / clear / remove missing | Del / Ctrl+Del / Ctrl+Shift+Del / Alt+Del | ⌫ / ⌘⌫ / ⇧⌘⌫ / ⌥⌫ (forward Delete also works) |
  | Top / bottom of the playlist | Home / End | also ⌘↑ / ⌘↓ |

  Option shortcuts stay as the Alt ones (⌥W, ⌥E, ⌥G…). Ctrl+click is a right click; Popamp's Ctrl+click actions are ⌘+click.
- **House style:** plain-English doc comments; platform behaviour behind `platform` or a clearly macOS module.
- **Where:** branch `macos/3-feel` from `macos/3-daily` in `/Users/mkovachev/sworkspace/popamp-macos`. Each task is one commit, with the `Claude-Session:` line the session asks for. Nothing goes upstream.

## Review Focus

1. **A key the table does not list** (⌘K, ⌘J, ⌘L, ⌘D, ⌥W…). Expected: it does exactly what Ctrl or Alt does on Linux, as the table's first rule says. Pinned by `command_does_what_ctrl_does_elsewhere` (Task 1).
2. **The Control key on a Mac.** Expected: Ctrl+letter does nothing (it isn't the main modifier), and Ctrl+click opens the right-click menu. Pinned by `control_is_not_the_main_modifier` and `control_click_is_a_right_click` (Task 1).
3. **Typing in Popamp's own text fields** (Preferences, the search boxes) **with ⌘ held.** Expected: no letter is typed for ⌘V, ⌘A and so on. Pinned by the `form_key` change and the manual check (Task 4).
4. **Pasting something that is neither a link nor a file** (a sentence, an empty clipboard). Expected: nothing is added. Pinned by `paste_takes_files_then_links_and_paths_and_nothing_else` (Task 2).
5. **Dragging a shaped window fast**, so the cursor leaves the shape mid-drag. Expected: the drag continues and the release is not lost (hit-testing never switches off while a button is down). Pinned by `the_cursor_is_over_a_shape_only_on_its_pixels` plus the drag guard, and the manual check (Task 4).

---

### Task 0: Branch

- [ ] `cd /Users/mkovachev/sworkspace/popamp-macos && git switch macos/3-daily && git switch -c macos/3-feel`. Expected: `Switched to a new branch 'macos/3-feel'`.

---

### Task 1: The Mac keymap and ⌘ labels

**Files:**
- Create: `apps/popamp/src/mac_keys.rs` (`#[cfg(any(target_os = "macos", test))] mod mac_keys;` in `main.rs`)
- Modify: `apps/popamp/src/app.rs`: `key()` (around line 1127), `input()` (line 1034), `window_event`'s `MouseInput`, `panel_event`'s `form_key` call (line 734)
- Modify: `apps/popamp/src/menu.rs:53-58`: `MenuItem::action`/`cmd` keep the label as given and translate the accel on macOS

**Interfaces:**
- `pub struct Mods { pub cmd: bool, pub ctrl: bool, pub alt: bool, pub shift: bool }`
- `pub enum Mac { Key { code: KeyCode, ctrl: bool, alt: bool, shift: bool }, Paste, Close, Minimize, Nothing }`
- `pub fn translate(code: KeyCode, m: Mods, window: WindowKey) -> Mac`. `Key` is a combination for the existing handler; `Nothing` swallows the key.
- `pub fn label(accel: &str) -> String`: a Linux/Windows accel (`"Ctrl+J"`, `"Ctrl+Shift+1"`, `"Alt+Del"`, `"Ctrl+Click"`) as a Mac one (`"⌘J"`, `"⌥⌘1"`, `"⌥⌫"`, `"⌘Click"`). Modifier order is ⌥⇧⌘, then the key.

- [ ] **Step 1: Write the failing tests** (`mac_keys.rs`)

```rust
fn key(code: KeyCode, ctrl: bool, alt: bool, shift: bool) -> Mac {
    Mac::Key { code, ctrl, alt, shift }
}
const CMD: Mods = Mods { cmd: true, ctrl: false, alt: false, shift: false };
const NONE: Mods = Mods { cmd: false, ctrl: false, alt: false, shift: false };

#[test]
fn command_does_what_ctrl_does_elsewhere() {
    for code in [KeyCode::KeyJ, KeyCode::KeyL, KeyCode::KeyD, KeyCode::KeyT, KeyCode::KeyK] {
        assert_eq!(translate(code, CMD, WindowKey::Main), key(code, true, false, false), "{code:?}");
    }
    assert_eq!(translate(KeyCode::KeyW, Mods { alt: true, ..NONE }, WindowKey::Main), key(KeyCode::KeyW, false, true, false));
    assert_eq!(translate(KeyCode::KeyX, NONE, WindowKey::Main), key(KeyCode::KeyX, false, false, false));
}

#[test]
fn the_mac_table() {
    let m = WindowKey::Main;
    let pl = WindowKey::Playlist;
    let with = |cmd, alt, shift| Mods { cmd, ctrl: false, alt, shift };
    assert_eq!(translate(KeyCode::Comma, CMD, m), key(KeyCode::KeyP, true, false, false));
    assert_eq!(translate(KeyCode::KeyW, CMD, m), Mac::Close);
    assert_eq!(translate(KeyCode::KeyM, CMD, m), Mac::Minimize);
    assert_eq!(translate(KeyCode::KeyW, with(true, false, true), m), key(KeyCode::KeyW, true, false, false));
    assert_eq!(translate(KeyCode::Backquote, CMD, m), key(KeyCode::Tab, true, false, false));
    assert_eq!(translate(KeyCode::KeyA, with(true, true, false), m), key(KeyCode::KeyA, true, false, false));
    assert_eq!(translate(KeyCode::KeyA, with(true, true, false), pl), key(KeyCode::KeyA, true, true, false));
    assert_eq!(translate(KeyCode::KeyA, CMD, pl), key(KeyCode::KeyA, true, false, false), "select all");
    assert_eq!(translate(KeyCode::KeyA, CMD, m), Mac::Nothing);
    assert_eq!(translate(KeyCode::KeyI, CMD, m), key(KeyCode::Digit3, false, true, false));
    assert_eq!(translate(KeyCode::KeyI, with(true, false, true), pl), key(KeyCode::KeyI, true, false, false));
    assert_eq!(translate(KeyCode::Digit2, with(true, true, false), pl), key(KeyCode::Digit2, true, false, true));
    assert_eq!(translate(KeyCode::KeyV, with(true, true, false), m), key(KeyCode::KeyV, true, false, false));
    assert_eq!(translate(KeyCode::KeyV, CMD, m), Mac::Paste);
    assert_eq!(translate(KeyCode::Backspace, NONE, pl), key(KeyCode::Delete, false, false, false));
    assert_eq!(translate(KeyCode::Backspace, CMD, pl), key(KeyCode::Delete, true, false, false));
    assert_eq!(translate(KeyCode::Backspace, with(true, false, true), pl), key(KeyCode::Delete, true, false, true));
    assert_eq!(translate(KeyCode::Backspace, Mods { alt: true, ..NONE }, pl), key(KeyCode::Delete, false, true, false));
    assert_eq!(translate(KeyCode::ArrowUp, CMD, pl), key(KeyCode::Home, false, false, false));
    assert_eq!(translate(KeyCode::ArrowDown, CMD, pl), key(KeyCode::End, false, false, false));
}

#[test]
fn control_is_not_the_main_modifier() {
    assert_eq!(translate(KeyCode::KeyJ, Mods { ctrl: true, ..NONE }, WindowKey::Main), Mac::Nothing);
}

#[test]
fn menus_show_mac_shortcuts() {
    for (linux, mac) in [
        ("Ctrl+J", "⌘J"), ("Ctrl+P", "⌘,"), ("Ctrl+W", "⇧⌘W"), ("Ctrl+A", "⌥⌘A"), ("Alt+3", "⌘I"),
        ("Ctrl+I", "⇧⌘I"), ("Ctrl+Shift+1", "⌥⌘1"), ("Ctrl+V", "⌥⌘V"), ("Del", "⌫"), ("Ctrl+Del", "⌘⌫"),
        ("Ctrl+Shift+Del", "⇧⌘⌫"), ("Alt+Del", "⌥⌫"), ("Alt+F4", "⌘Q"), ("Ctrl+Click", "⌘Click"),
        ("Shift+Click", "⇧Click"), ("Alt+W", "⌥W"), ("Ctrl+Alt+I", "⌥⌘I"), ("Shift+R", "⇧R"), ("R", "R"),
    ] {
        assert_eq!(label(linux), mac, "{linux}");
    }
}
```

Also, in `app.rs` tests, a mouse rule:

```rust
#[cfg(target_os = "macos")]
#[test]
fn control_click_is_a_right_click() {
    assert_eq!(mac_keys::button(MouseButton::Left, mac_keys::Mods { cmd: false, ctrl: true, alt: false, shift: false }), MouseButton::Right);
    assert_eq!(mac_keys::button(MouseButton::Left, mac_keys::Mods { cmd: true, ctrl: false, alt: false, shift: false }), MouseButton::Left);
}
```

(`pub fn button(b: MouseButton, m: Mods) -> MouseButton` lives in `mac_keys.rs`.)

- [ ] **Step 2:** Run `cargo test -p popamp mac_keys` and confirm it fails (the module doesn't exist).
- [ ] **Step 3: Implement `mac_keys.rs`** as the table says.
  - Physical Ctrl without ⌘ gives `Nothing` for letters, digits and named keys. Plain keys and Option combinations pass through (with `alt`).
  - `label` handles the exceptions from the table first, then the generic Ctrl/Alt/Shift tokens.
  - Doc comments name the decision: Mac first.
- [ ] **Step 4: Wire it in, behind `cfg(target_os = "macos")`.**
  - **`key()`:** right after `let (ctrl, shift, alt) = …`, build `Mods` from winit's modifiers (`super_key()` is ⌘), then:
    - `Paste` calls the paste of Task 2. Until then, `Nothing`.
    - `Close` runs `C::Exit` for the main window, `C::ToggleEqWindow` for the equalizer, `C::TogglePlaylistWindow` for the playlist.
    - `Minimize` runs `C::Minimize`.
    - `Nothing` returns.
    - `Key` replaces `code`, `ctrl`, `alt`, `shift` for the rest of the existing function. The playlist block reads `code` and the modifiers, so it must see the replaced ones.
  - **Clicks:** `input()` takes `ctrl` from `super_key()`, and `MouseInput` maps the button through `mac_keys::button`.
  - **Text fields:** `form_key`'s shortcut flag also counts ⌘.
  - **Menus:** `MenuItem::action` stores `mac_keys::label(accel)` on macOS.
- [ ] **Step 5:** Run `cargo test -p popamp` and clippy on all three feature sets. Expected: pass. Menu tests that compare accel text run on Linux and Windows unchanged; on macOS, adjust them only if they compare accels, by asserting through `label`.
- [ ] **Step 6: Commit:** `Use the Mac's keys on macOS: ⌘ first, and menus that show them`.

---

### Task 2: ⌘V pastes links and files into the playlist

**Files:**
- Modify: `apps/popamp/Cargo.toml` (macOS: `objc2-app-kit = { version = "0.3.2", default-features = false, features = ["std", "NSPasteboard"] }`, with the features its types need: check with `cargo info`), `Cargo.lock`
- Modify: `apps/popamp/src/mac_keys.rs` (pure `paste_items`), `apps/popamp/src/platform/macos.rs` (`Platform::clipboard`), `apps/popamp/src/app.rs` (`Mac::Paste`)

**Interfaces:**
- `pub fn paste_items(files: Vec<String>, text: Option<String>) -> Vec<String>`: the file paths if there are any. Otherwise each trimmed line of `text` that starts with `http://` or `https://` or names an existing file or folder. Order is kept, and nothing else gets through.
- `Platform::clipboard(&self) -> (Vec<String>, Option<String>)` (macOS): `NSPasteboard::generalPasteboard()`, file URLs read as `NSURL` objects (only `isFileURL`, their `path`), and the plain-text string.
- In `App`, `Mac::Paste` gives `paste_items(...)`; if that is not empty, `self.model.on_remote(vec![Remote::Add(items)])`, then `after_model` and redraw.

- [ ] **Step 1: Write the failing test** (`mac_keys.rs`)

```rust
#[test]
fn paste_takes_files_then_links_and_paths_and_nothing_else() {
    let tmp = std::env::temp_dir();
    let dir = tmp.to_string_lossy().into_owned();
    assert_eq!(paste_items(vec!["/a.mp3".into()], Some("https://x".into())), ["/a.mp3"]);
    let text = format!("https://ice1.somafm.com/groovesalad-128-mp3\n\n  {dir}  \nhello there\n/popamp/none.mp3\nftp://x");
    assert_eq!(paste_items(vec![], Some(text)), ["https://ice1.somafm.com/groovesalad-128-mp3".to_string(), dir]);
    assert!(paste_items(vec![], None).is_empty());
}
```

- [ ] **Step 2:** Run it and confirm it fails (`paste_items` missing).
- [ ] **Step 3: Implement** `paste_items`, `Platform::clipboard`, and the `Mac::Paste` handling.
- [ ] **Step 4:** Run the tests and clippy. Expected: pass.
- [ ] **Step 5: Commit:** `Paste copied links and files into the playlist with ⌘V on macOS`.

---

### Task 3: Shaped skins, with clicks passing through

**Files:**
- Modify: `apps/popamp/Cargo.toml` (macOS: `objc2-quartz-core` 0.3.2 `["std", "objc2-core-foundation", "CALayer", "CATransaction"]`, `objc2-core-graphics` 0.3.2 `["std", "CGImage", "CGColorSpace", "CGDataProvider", "CGEvent"]`, `objc2-core-foundation` adds `CFCGTypes` if not already there), `Cargo.lock`
- Modify: `apps/popamp/src/platform/mod.rs` (`Mask::covers`), `apps/popamp/src/platform/macos.rs` (mask layer, kept masks, `track_cursor`), `platform/x11.rs` and `platform/windows.rs` (no-op `track_cursor`, `tracks_cursor() -> false`), `apps/popamp/src/app.rs` (call `track_cursor` each tick; tick at least 30 fps while `tracks_cursor()`)

**Interfaces:**
- `impl Mask { pub fn covers(&self, x: i32, y: i32) -> bool }`. `(x, y)` is in the window's desktop units; it uses `scale.skin(x)` and `scale.skin(y)`, and anything out of range is false.
- `Platform::set_shape` (macOS):
  - Windows of roles Main, Docked and Bar are created `with_transparent(true)`.
  - The view layer's `mask` becomes a `CALayer` whose contents are a premultiplied-alpha `CGImage` built from `shape_rects` (as in the spike patch), then the shadow is invalidated.
  - The `Mask` is kept by window ID, with the window's `Weak` or ID and size.
  - `None` removes the mask layer and the kept mask, and turns hit-testing back on.
- `Platform::track_cursor(&self, windows: &[&Window], button_down: bool)` (macOS):
  - Read the global cursor in points (`CGEvent::new(None)` then `CGEvent::location`). Its origin is the top-left of the main screen, the same as winit's positions in points.
  - For each shaped window, work out the cursor relative to `platform::from_screen(inner_position, scale_factor)`, then `set_cursor_hittest(mask.covers(..))`.
  - Never switch hit-testing off while `button_down`.
  - `tracks_cursor(&self) -> bool` is true while any mask is kept.
- In `App`, on each tick call `track_cursor` with the skin windows and `self.drag.is_some() || self.capture.is_some()`.

- [ ] **Step 1: Write the failing test** (`platform/mod.rs` tests; this runs on every system)

```rust
#[test]
fn the_cursor_is_over_a_shape_only_on_its_pixels() {
    // A 4x2 rectangle at (1, 0) in a 6x3 window, shown at x2.
    let rect = vec![vec![(1, 0), (5, 0), (5, 2), (1, 2)]];
    let m = region(&rect, 6, 3, Scale::new(200, Scaling::Sharp));
    assert!(m.covers(2, 0) && m.covers(9, 3));
    assert!(!m.covers(1, 0), "left of the shape");
    assert!(!m.covers(4, 5), "below it");
    assert!(!m.covers(-1, 0) && !m.covers(100, 1), "outside the window");
}
```

- [ ] **Step 2:** Run `cargo test -p popamp the_cursor_is_over` and confirm it fails (`covers` missing).
- [ ] **Step 3: Implement.** Port the spike's mask-layer code into `platform/macos.rs` behind `set_shape`. Add the kept masks, `track_cursor`, `tracks_cursor`, and the app's per-tick call. Doc comments say why: macOS has no window shapes, and AppKit doesn't pass clicks through a masked layer.
- [ ] **Step 4:** Run the tests and clippy on all three feature sets. Expected: pass.
- [ ] **Step 5: Commit:** `Cut skins to their shape on macOS, with clicks passing through the cut-out parts`.

---

### Task 4: The whole workspace, and the Mac feel by hand

- [ ] **Step 1:** Run all checks on all three feature sets. Expected: clean and passing.
- [ ] **Step 2: Rebuild the development bundle.** Run `sh packaging/macos/build-dev-app.sh`, then with the contributor:
  - **Keys:** ⌘J, ⌘L, ⌘D, ⌘, (Preferences), ⇧⌘W (windowshade), ⌘` (cycle), ⌥⌘A (always on top), ⌘I (file info), ⌫ (remove from the playlist), ⌘↑/⌘↓, ⌘W on the equalizer (closes it), ⌘M (minimize), ⌘Q (quits, and the playlist is there next time).
  - **Clicks and text:** Ctrl+click opens the menu. Typing in Preferences with ⌘ held types nothing.
  - **Menus:** they show ⌘ shortcuts.
  - **Paste:** ⌘V with SomaFM's link copied adds it; with files copied in Finder it adds them; with a sentence copied it adds nothing.
  - **Shapes:** the round test skin, then a classic skin with a shape from the Skin Museum (Skins → Browse; the contributor picks one).
    - Clicks outside the shape reach the window behind.
    - Clicks inside work, and a fast drag doesn't drop.
- [ ] **Step 3:** Push and open the stacked draft PR: `gh pr create -R miraclebg/popamp --base macos/3-daily --head macos/3-feel --draft --title "macOS: Mac keys, paste, shaped skins (stage 3b)"`, with the session line in the body. CI (Linux, Windows, MSRV) green.
