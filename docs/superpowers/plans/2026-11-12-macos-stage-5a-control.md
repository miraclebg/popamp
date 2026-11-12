# macOS Stage 5a: Control Popamp from Anywhere

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On a Mac, the media keys, AirPods and Control Center's Now Playing control Popamp and show the track and cover. A menu-bar icon carries the tray's menu, and ⌃⌥ global hotkeys work from any app. All of this sits on top of upstream 0.9 merged into the fork.

**Architecture:** Each part fills a macOS placeholder from stage 1 behind 0.9's existing interfaces:
- `media/nowplaying.rs` behind `service::spawn` (fed by `Snapshot`s, as MPRIS is);
- `media/tray_macos.rs` behind `Tray::{new, update}` (with the shared `tray_menu`);
- the macOS `os` module of `hotkeys/mod.rs` behind `os::grab`.

AppKit and MediaPlayer calls go to the main queue through `dispatch2`. Remote commands go back through the existing `Sender`/`send`.

**Tech Stack:** Rust 1.96+, `objc2` 0.6, `objc2-media-player` 0.3.2, `objc2-app-kit` 0.3.2, `objc2-foundation` 0.3.2, `block2` 0.6, `dispatch2` 0.3, and Carbon's hotkey API through a few `extern "C"` declarations (HIToolbox, linked as the `Carbon` framework).

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md` (branch `macos-port-design`), "Stage 5, as approved on 2026-11-12 (after 0.9)".

## Global Constraints

- No behaviour change on Linux or Windows; new dependencies are macOS-only, `default-features = false`, with only the features used.
- fmt, `clippy -D warnings` and tests on default, `popamp/plugins` and `popamp/plus`; `Cargo.lock` committed.
- **No Claude notices** in commit messages, PR descriptions or comments: no session links, no `Claude-Session:` trailers (the contributor's instruction).
- House style: plain-English doc comments; one module per system behind the existing interface.
- AppKit and MediaPlayer objects are touched only on the main thread. The media service's and the tray's threads hand work to `DispatchQueue::main()`.
- Global hotkeys need no Accessibility permission (Carbon `RegisterEventHotKey`, not an event tap).
- Branch `macos/5a-control` from the fork's `main` with upstream `main` (0.9) merged in, in `/Users/mkovachev/sworkspace/popamp-macos`. One commit per task. Nothing goes upstream.

## Review Focus

1. **Another app takes over Now Playing** (Music or a browser plays). Expected: Popamp stops answering the media keys and gets them back when it plays again, as macOS does for every app. Pinned by the manual check (Task 4).
2. **Play/Pause pressed while stopped.** Expected: playback starts (Play), and does not toggle into "paused". Pinned by `the_media_keys_ask_what_the_buttons_ask` (Task 1).
3. **Turning the tray setting off and on while Popamp runs.** Expected: the menu-bar icon goes away and comes back, with no second icon. Pinned by the manual check (Task 4) and `crate::hold::update`'s tests.
4. **A ⌃⌥ hotkey another app already owns.** Expected: Popamp leaves it to that app and logs one line, as 0.9 does on Linux and Windows. Pinned by the registration error path in Task 3.
5. **A stream** (radio): Now Playing shows the stream title and no length, and seeking is not offered. Pinned by `a_stream_has_no_length_to_seek_in` (Task 1).

---

### Task 0: 0.9 under the fork's macOS work

**Files:** a merge commit, plus the stage-1-on-0.9 resolutions.

- [ ] **Step 1: Merge.** `git fetch origin upstream && git switch -c macos/5a-control origin/main && git merge upstream/main`.
- [ ] **Step 2: Resolve the conflicts.**
  - For `apps/popamp/src/media/mod.rs`, take the resolved file from `macos/stage-1` (`git show macos/stage-1:apps/popamp/src/media/mod.rs`), which already gates 0.9's notify and tray for macOS and adds the `ps` fallback.
  - Bring over `media/tray_macos.rs` and the hotkeys commit (`git cherry-pick 2de0b84`, or the same edit by hand).
  - Keep the fork's later stages: `nowplaying.rs` is still the stage-1 stub at this point.
- [ ] **Step 3: Check everything.**
  - Run fmt, clippy and tests on the three feature sets. Expected: all green.
  - `cargo run --release -p popamp -- /NEW /INIDIR=/tmp/popamp-5a`: it starts and plays a file (CoreAudio from stage 2).
- [ ] **Step 4: Commit** the merge, with a message that says it brings 0.9 under the macOS work and names the placeholders added for its tray, notifications and hotkeys.

---

### Task 1: Media keys and Now Playing

**Files:** Modify `apps/popamp/Cargo.toml` (macOS: `objc2-media-player` with features `MPNowPlayingInfoCenter`, `MPRemoteCommandCenter`, `MPRemoteCommand`, `MPRemoteCommandEvent`, `MPMediaItem`, plus `block2`, `dispatch2`, and the `objc2-app-kit` `NSImage` feature for artwork). Modify `apps/popamp/src/media/nowplaying.rs`. In `media/mod.rs`, the module-wide macOS `allow(dead_code)` goes if nothing is left unused.

**Interfaces:**
- Keeps `pub fn spawn(proxy: EventLoopProxy<AppEvent>, shared: Arc<Shared>, instance: &str) -> Option<crossbeam_channel::Sender<Snapshot>>`.
- Pure, for tests:
  - `fn info(snap: &Snapshot, position_ms: u64) -> Option<Info>`, with `struct Info { title: String, artist: Option<String>, album: Option<String>, length_s: Option<f64>, elapsed_s: f64, rate: f64 }`. `None` when there is no track; `rate` is 1.0 while playing and 0.0 otherwise.
  - `fn remotes(command: Mac, status: &str) -> Vec<Remote>`, with `enum Mac { Play, Pause, Toggle, Stop, Next, Previous, Seek(f64) }`:
    - Toggle gives `Cmd(Pause)` while playing and `Cmd(Play)` otherwise;
    - Pause uses the shared `super::pause(status)`;
    - Seek gives `Remote::SeekMs` in milliseconds.

- [ ] **Step 1: Write the failing tests** (`nowplaying.rs`)

```rust
fn snap(status: &'static str, length_ms: Option<u64>) -> Snapshot {
    let track = Track { id: 1, title: "Song".into(), artist: Some("Band".into()), album: None, url: "/a.mp3".into(), length_ms };
    Snapshot { status, track: Some(track), ..Snapshot::default() }
}

#[test]
fn now_playing_shows_the_track_where_it_is() {
    let i = info(&snap("Playing", Some(200_000)), 61_500).unwrap();
    assert_eq!((i.title.as_str(), i.artist.as_deref(), i.length_s, i.elapsed_s, i.rate), ("Song", Some("Band"), Some(200.0), 61.5, 1.0));
    assert_eq!(info(&snap("Paused", Some(200_000)), 0).unwrap().rate, 0.0);
    assert!(info(&Snapshot::default(), 0).is_none());
}

#[test]
fn a_stream_has_no_length_to_seek_in() {
    assert_eq!(info(&snap("Playing", None), 5_000).unwrap().length_s, None);
}

#[test]
fn the_media_keys_ask_what_the_buttons_ask() {
    assert_eq!(remotes(Mac::Toggle, "Playing"), [Remote::Cmd(Command::Pause)]);
    assert_eq!(remotes(Mac::Toggle, "Stopped"), [Remote::Cmd(Command::Play)]);
    assert_eq!(remotes(Mac::Toggle, "Paused"), [Remote::Cmd(Command::Play)]);
    assert!(remotes(Mac::Pause, "Paused").is_empty(), "pausing a paused player would resume it");
    assert_eq!(remotes(Mac::Seek(12.25), "Playing"), [Remote::SeekMs(12_250)]);
    assert_eq!(remotes(Mac::Next, "Playing"), [Remote::Cmd(Command::Next)]);
}
```

(Check `Track`'s 0.9 fields and `Remote::SeekMs`'s type, and adjust the literals to them.)

- [ ] **Step 2:** Run it and confirm it fails.
- [ ] **Step 3: Implement.**
  - **Commands:** on `spawn`, on the main queue, register handlers on `MPRemoteCommandCenter::sharedCommandCenter()` for play, pause, togglePlayPause, stop, nextTrack, previousTrack and changePlaybackPosition (the event's `positionTime`). Each handler maps its command through `remotes` with the last status it saw (kept in an `Arc<Mutex<&'static str>>`), calls `super::send(&proxy, …)`, and returns success.
  - **Snapshots:** a thread reads them, as MPRIS's does, works out `info` with `shared.position_ms()`, and on the main queue sets `MPNowPlayingInfoCenter::defaultCenter()`'s `nowPlayingInfo`:
    - title, artist, album;
    - duration only when known;
    - elapsed and rate;
    - artwork from `super::Covers::of(track)` as an `MPMediaItemArtwork` over an `NSImage` from that file;
    - and `playbackState` from the status.
  - **Seeking:** enable `changePlaybackPositionCommand` only when the track has a length.
  - **Stopping:** when the channel closes, clear `nowPlayingInfo` and remove the targets.
- [ ] **Step 4:** Run the tests and clippy on the three feature sets. Expected: pass.
- [ ] **Step 5: Commit:** `Answer the Mac's media keys and show the track in Now Playing`.

---

### Task 2: The menu-bar icon

**Files:** Modify `apps/popamp/Cargo.toml` (macOS `objc2-app-kit` features `NSStatusBar`, `NSStatusItem`, `NSStatusBarButton`, `NSButton`, `NSControl`, `NSView`, `NSResponder`, `NSMenu`, `NSMenuItem`, `NSImage`, `NSCell`, as the crate needs) and `apps/popamp/src/media/tray_macos.rs`.

**Interfaces:**
- Keeps `pub struct Tray`, `Tray::new(send: Sender) -> Tray` and `Tray::update(&mut self, snap: &Snapshot)`.
- `update` shows or hides through `crate::hold::update`, as `tray_windows.rs` does, and rebuilds the menu from `super::tray_menu(snap)` when it changed.
- Every AppKit call runs on the main queue. The `NSStatusItem` lives on the main thread, in a `thread_local!` there.

- [ ] **Step 1: Write the failing test** (`tray_macos.rs`; the pure part)

```rust
#[test]
fn the_menu_bar_shows_what_plays() {
    assert_eq!(tooltip(&Snapshot::default()), "Popamp\nStopped");
}
```

(`fn tooltip(snap) -> String` = `"Popamp\n" + super::now_playing(snap)`, as the other trays word it.)

- [ ] **Step 2:** Run it and confirm it fails.
- [ ] **Step 3: Implement.**
  - **The item:** `NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength)`.
  - **Its image:** a template image of the SF Symbol `music.note` (`NSImage::imageWithSystemSymbolName_accessibilityDescription`), falling back to the title "♫". Template means it follows the menu bar's light or dark look. The button's tooltip is `tooltip(snap)`.
  - **The menu:** an `NSMenu` built from `tray_menu`:
    - separators for `None`;
    - check marks from `checked`;
    - each item targets one receiver object (`define_class!`, kept alive with the item) with an `action:` selector and its index as `tag`;
    - the receiver sends `entries[tag].remote` through the `Sender`.
  - **Hiding:** removes the item from the status bar.
- [ ] **Step 4:** Run the tests and clippy. Expected: pass.
- [ ] **Step 5: Commit:** `Show Popamp's tray menu in the Mac menu bar`.

---

### Task 3: Global hotkeys

**Files:** Modify `apps/popamp/src/hotkeys/mod.rs` (the macOS `os` module; the macOS `allow(dead_code)` on `Key` and `BINDINGS` goes).

**Interfaces:**
- Keeps `pub struct Grab` and `pub fn grab(send: Send) -> Option<Grab>`, called on the main thread from `Hotkeys::want`.
- Pure: `fn mac_keycode(key: Key) -> u32`, the virtual key codes:
  - Insert → `kVK_Help` 0x72 (its place on Mac keyboards);
  - Home 0x73, End 0x77, Page Up 0x74, Page Down 0x79;
  - Left 0x7B, Right 0x7C, Down 0x7D, Up 0x7E.
- Modifiers: `controlKey | optionKey` (0x1000 | 0x0800).

- [ ] **Step 1: Write the failing test** (in the hotkeys tests; it becomes the first macOS test there, so the module's `not(target_os = "macos")` gate goes)

```rust
#[cfg(target_os = "macos")]
#[test]
fn each_hotkey_is_a_mac_key() {
    assert_eq!(os::mac_keycode(Key::Home), 0x73);
    assert_eq!(os::mac_keycode(Key::Insert), 0x72);
    assert_eq!(os::mac_keycode(Key::Right), 0x7C);
    let codes: std::collections::HashSet<u32> = BINDINGS.iter().map(|(k, _)| os::mac_keycode(*k)).collect();
    assert_eq!(codes.len(), BINDINGS.len(), "no two keys share a code");
}
```

- [ ] **Step 2:** Run it and confirm it fails.
- [ ] **Step 3: Implement** with a few `extern "C"` declarations from HIToolbox (`#[link(name = "Carbon", kind = "framework")]`): `GetApplicationEventTarget`, `InstallEventHandler` (for `kEventClassKeyboard`/`kEventHotKeyPressed`), `GetEventParameter` (`kEventParamDirectObject`, `typeEventHotKeyID`), `RegisterEventHotKey`, `UnregisterEventHotKey` and `RemoveEventHandler`.
  - `grab` installs one handler, with a boxed `Send` as its user data.
  - It registers each of `BINDINGS` with id `n` and signature `'popa'`. A key that fails to register (another app owns it) is logged once and skipped.
  - The handler maps the pressed id to `BINDINGS[n].1` and calls `send`.
  - Dropping `Grab` unregisters the keys, removes the handler, and frees the box.
  - The handler runs on the main thread from the app's run loop.
  - Doc comments say why Carbon: no Accessibility prompt, which an event tap would need.
- [ ] **Step 4:** Run the tests and clippy on the three feature sets. Expected: pass.
- [ ] **Step 5: Commit:** `Grab the global hotkeys on macOS, with ⌃⌥ as Ctrl+Alt elsewhere`.

---

### Task 4: By hand, and the draft PR

- [ ] **Step 1:** Run all checks on all three feature sets, then `sh packaging/macos/build-dev-app.sh`.
- [ ] **Step 2: With the contributor:**
  - **Media keys:** ⏯ ⏭ ⏮ on the keyboard play, pause and skip Popamp; so do AirPods (if at hand).
  - **Control Center → Now Playing:** shows the title, artist and cover; the progress bar moves; dragging it seeks. On SomaFM: the stream title, no length, no seeking.
  - **Handover:** start Music; the keys go to Music; play Popamp again and they come back.
  - **The tray:** with it turned on (Preferences or the setting), the menu-bar note shows the track on hover. The menu works (Show, transport, Shuffle/Repeat checks, Exit). Turning it off removes it.
  - **Global hotkeys:** with them turned on, ⌃⌥→ goes to the next track and ⌃⌥Home plays, from another app in front.
- [ ] **Step 3:** Push and open the draft PR in the fork: `gh pr create -R miraclebg/popamp --base main --head macos/5a-control --draft --title "macOS: media keys, Now Playing, menu-bar icon, global hotkeys (stage 5a)"`, with a body that has no Claude notice. Expected: CI green.
