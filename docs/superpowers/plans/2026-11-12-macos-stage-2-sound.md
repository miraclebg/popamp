# macOS Stage 2: Popamp Plays Sound on macOS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Stage 1 placeholder in `popamp-audio/src/output/coreaudio.rs` with real CoreAudio output behind the same `Pcm` interface, so files and radio streams play on a Mac. Then run the shaped-skin spike.

**Architecture:** One output AudioUnit per open stream: Apple's *default output* unit for `default` (it follows the system's default device), the *HAL output* unit bound to a device for a chosen device UID. The unit's render callback reads interleaved `f32` frames from an `rtrb` single-producer, single-consumer ring sized from `latency_ms`. The callback takes the consumer through `try_lock` and plays silence for anything missing, so it never waits and never allocates. `write` blocks by napping while the ring is full. Device lists and properties come from the HAL (`AudioObjectGetPropertyData`).

Two narrowings of the spec's Stage 2, decided here:
- `write` naps rather than being woken by a signal from the callback. The callback then touches nothing but the ring, and a nap of a quarter of the device buffer costs no audible latency.
- `delay` is 0 once the queue is empty rather than counting the device's own few milliseconds. That error is under the device latency, about 15 ms, and invisible on the time display. In exchange, `drain` and the engine's end of track agree on "nothing left to hear".

The throwaway spike (`scratchpad/caspike`, outside the repo) already confirmed on the contributor's Mac:
- these crates and features build;
- device names and UIDs read correctly;
- the default output unit calls back in real time at 44.1 kHz, converting to the device rate.

**Tech Stack:** Rust 1.96+, `objc2-audio-toolbox` 0.3.2, `objc2-core-audio` 0.3.2, `objc2-core-audio-types` 0.3.2, `objc2-core-foundation` 0.3.2 (the `objc2` 0.6 family already in `Cargo.lock` through winit), `rtrb` 0.4 (MIT OR Apache-2.0, no dependencies), `parking_lot` (already a dependency).

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md` (branch `macos-port-design`): "Stage 2: it plays" and the "Shaped-skin spike" right after it.

## Global Constraints

- macOS 12 or newer, Apple Silicon first. No behaviour change on Linux or Windows; the new dependencies are macOS-only (`[target.'cfg(target_os = "macos")'.dependencies]`, `default-features = false`).
- `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked`, each with default features, `--features popamp/plugins` and `--features popamp/plus`. `Cargo.lock` is committed with the change (CI builds `--locked`).
- Real-time safety (CONTRIBUTING.md): the render callback takes no blocking lock, allocates nothing, makes no system call beyond the AudioUnit's own.
- House style: plain-English doc comments; one module per system behind `output/mod.rs`'s interface; the same names and semantics as `alsa.rs`/`wasapi.rs` (`open`, `write`, `delay`, `pause`, `discard`, `drain`, `pub rate`, `pub channels`, `available`, `devices`).
- Settings keep a device by its **UID** (`kAudioDevicePropertyDeviceUID`), which stays the same across reboots; people read its **name**.
- Work happens on branch `macos/2-sound`, made from `macos/1-build` in the worktree `/Users/mkovachev/sworkspace/popamp-macos`. The spec and plans stay on `macos-port-design`.
- Tests that need a sound device skip themselves, with a note on stderr, when the Mac has no default output device (CI runners may have none).
- Nothing goes upstream in this plan.

## Review Focus

1. **The user switches output device in Preferences while a track plays** (MacBook speakers to the LG monitor). Expected: playback goes on from the same position on the new device, as `set_device` does on Linux. Pinned by `a_device_opens_by_its_uid` (Task 1) and the manual check in Task 3, Step 3.
2. **A device chosen in Preferences, then unplugged** (headphones, the monitor). Expected: opening it fails with a message naming it, and the engine's existing fallback plays on the default device. Pinned by `an_unknown_device_is_named_in_the_error` (Task 1).
3. **Pause, then seek, then play.** Expected: no burst of old audio after the seek (the queue was emptied), and the position shown is right. Pinned by `discard_empties_the_queue` (Task 2).
4. **A radio stream whose rate differs from the device's** (a 44.1 kHz stream on a 48 kHz device). Expected: correct pitch and speed; the AudioUnit converts. Pinned by `the_default_device_plays_in_real_time` at 44.1 kHz (Task 2) and the radio check in Task 3, Step 3.
5. **The last second of a track.** Expected: it is heard before the next track starts or playback stops (`drain` waits for the queue and the device latency). Pinned by `drain_waits_until_everything_is_heard` (Task 2).

---

### Task 0: Branch

- [ ] **Step 1: Make the branch**

Run: `cd /Users/mkovachev/sworkspace/popamp-macos && git switch macos/1-build && git switch -c macos/2-sound`
Expected: `Switched to a new branch 'macos/2-sound'`.

---

### Task 1: Devices: list, find by UID, report what exists

**Files:**
- Modify: `crates/popamp-audio/Cargo.toml` (new macOS dependency section), `Cargo.lock`
- Modify: `crates/popamp-audio/src/output/coreaudio.rs` (keep the `null` path; replace the "not yet" parts)

**Interfaces:**
- Produces (private to `coreaudio.rs`, used by Task 2):
  - `fn default_output() -> Option<AudioObjectID>` (`kAudioHardwarePropertyDefaultOutputDevice`; `None` for `kAudioObjectUnknown`);
  - `fn output_devices() -> Vec<(AudioObjectID, String /* uid */, String /* name */)>` (every device of `kAudioHardwarePropertyDevices` with a non-empty `kAudioDevicePropertyStreams` in `kAudioObjectPropertyScopeOutput`);
  - `fn device_by_uid(uid: &str) -> Option<AudioObjectID>`;
  - `fn device_latency(id: AudioObjectID) -> (u32 /* frames */, f64 /* device rate */)`: `kAudioDevicePropertyLatency` + `kAudioDevicePropertySafetyOffset` (output scope) + `kAudioDevicePropertyBufferFrameSize` (global), and `kAudioDevicePropertyNominalSampleRate`;
  - `fn to_stream_frames(device_frames: u32, device_rate: f64, rate: u32) -> u64`.
- Produces (public, unchanged signatures): `available() -> Result<(), String>`, `devices() -> Vec<Device>` with `Device { name: uid, label: name }`.

- [ ] **Step 1: Add the macOS dependencies**

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2-audio-toolbox = { version = "0.3.2", default-features = false, features = ["std", "AUComponent", "AudioComponent", "AudioOutputUnit", "AudioUnitProperties", "objc2-core-audio-types"] }
objc2-core-audio = { version = "0.3.2", default-features = false, features = ["std", "AudioHardware", "objc2-core-audio-types"] }
objc2-core-audio-types = { version = "0.3.2", default-features = false, features = ["std", "CoreAudioBaseTypes"] }
objc2-core-foundation = { version = "0.3.2", default-features = false, features = ["std", "CFString"] }
rtrb = "0.4"
```

Run: `cargo check -p popamp-audio` → builds; `git diff --stat Cargo.lock` shows only additions.

- [ ] **Step 2: Write the failing tests** (in `coreaudio.rs` tests; replace `other_devices_say_macos_has_no_sound_yet`; keep `null_takes_samples_and_plays_nothing`)

```rust
/// Whether this Mac can play sound; hardware tests skip themselves otherwise.
fn speakers() -> bool {
    let there = default_output().is_some();
    if !there {
        eprintln!("no sound output device: skipped");
    }
    there
}

#[test]
fn device_frames_count_at_the_streams_rate() {
    assert_eq!(to_stream_frames(480, 48_000.0, 44_100), 441);
    assert_eq!(to_stream_frames(512, 44_100.0, 44_100), 512);
    assert_eq!(to_stream_frames(0, 0.0, 44_100), 0); // a device that says nothing
}

#[test]
fn devices_have_a_name_people_read_and_a_uid_settings_keep() {
    if !speakers() {
        return;
    }
    let list = devices();
    assert!(!list.is_empty() && available().is_ok());
    assert!(list.iter().all(|d| !d.label.is_empty() && !d.name.is_empty() && d.name != super::super::DEFAULT));
    let mut uids: Vec<&str> = list.iter().map(|d| d.name.as_str()).collect();
    uids.dedup();
    assert_eq!(uids.len(), list.len(), "{list:?}");
}

#[test]
fn a_device_opens_by_its_uid() {
    if !speakers() {
        return;
    }
    let first = devices().remove(0);
    assert!(CoreAudioPcm::open(&first.name, 48_000, 2, 150).is_ok(), "{first:?}");
}

#[test]
fn an_unknown_device_is_named_in_the_error() {
    let e = CoreAudioPcm::open("popamp-no-such-uid", 48_000, 2, 150).err().unwrap();
    assert!(e.contains("popamp-no-such-uid"), "{e}");
}
```

- [ ] **Step 3: Run to see them fail**

Run: `cargo test -p popamp-audio output::coreaudio 2>&1 | tail -5`
Expected: compile errors (`default_output`, `to_stream_frames` missing).

- [ ] **Step 4: Implement the device functions**

Use `AudioObjectGetPropertyDataSize` + `AudioObjectGetPropertyData` with `AudioObjectPropertyAddress { mSelector, mScope, mElement: kAudioObjectPropertyElementMain }` on `kAudioObjectSystemObject`. Read string properties as a `*const CFString` and take ownership with `CFRetained::from_raw` (the HAL hands out a +1 reference), then `to_string()`. `to_stream_frames` is `(device_frames as f64 * rate as f64 / device_rate).round() as u64`, or 0 when `device_rate <= 0`. `available()` is `Err("no sound output device")` when `default_output()` is `None`. `open` keeps `null`, returns `Err(format!("no output device {device}"))` for a UID `device_by_uid` cannot find, and for now `Err("not yet")` for a found device and `default` (Task 2 opens them).

`a_device_opens_by_its_uid` still fails after this step; that is expected and Task 2 makes it pass. The other three pass.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p popamp-audio output::coreaudio`
Expected: everything passes except `a_device_opens_by_its_uid`.

- [ ] **Step 6: Commit**

```bash
git add crates/popamp-audio/Cargo.toml Cargo.lock crates/popamp-audio/src/output/coreaudio.rs
git commit -m "List the Mac's sound devices by name, kept by their UID"
```

---

### Task 2: The stream: play, queue, pause, discard, drain

**Files:**
- Modify: `crates/popamp-audio/src/output/coreaudio.rs`
- Modify: `crates/popamp-audio/src/engine.rs` (tests: replace `without_sound_output_a_track_fails_instead_of_racing`)

**Interfaces:**
- Consumes: Task 1's `default_output`, `device_by_uid`, `device_latency`, `to_stream_frames`.
- Produces: `CoreAudioPcm { stream: Option<Stream>, pub rate: u32, pub channels: u32 }` (`None` is `null`), with the `output::Pcm` methods. Private: `struct Stream`, `struct Shared { consumer: parking_lot::Mutex<rtrb::Consumer<f32>> }`, `unsafe extern "C-unwind" fn render(NonNull<c_void>, NonNull<AudioUnitRenderActionFlags>, NonNull<AudioTimeStamp>, u32, u32, *mut AudioBufferList) -> i32`, and `fn ring_capacity(rate: u32, channels: u32, latency_ms: u32, device_buffer_frames: u32) -> usize`.

- [ ] **Step 1: Write the failing tests**

`coreaudio.rs` tests:

```rust
#[test]
fn the_queue_holds_the_latency_asked_for_or_two_device_buffers() {
    assert_eq!(ring_capacity(48_000, 2, 150, 512), 7_200 * 2);
    assert_eq!(ring_capacity(48_000, 2, 10, 512), 1_024 * 2);
}

/// `ms` of stereo silence at `rate`.
fn silence(rate: u32, ms: u32) -> Vec<f32> {
    vec![0.0; (rate * ms / 1000 * 2) as usize]
}

#[test]
fn the_default_device_plays_in_real_time() {
    if !speakers() {
        return;
    }
    // 44.1 kHz on what is usually a 48 kHz device: the unit converts.
    let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 44_100, 2, 100).unwrap();
    let started = std::time::Instant::now();
    for _ in 0..10 {
        pcm.write(&silence(44_100, 50)).unwrap();
    }
    let took = started.elapsed();
    assert!(took >= std::time::Duration::from_millis(300) && took < std::time::Duration::from_secs(2), "{took:?}");
}

#[test]
fn delay_counts_what_is_queued() {
    if !speakers() {
        return;
    }
    let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 150).unwrap();
    assert!(pcm.pause(true));
    pcm.write(&silence(48_000, 50)).unwrap();
    assert!(pcm.delay() >= 2_400, "{}", pcm.delay());
}

#[test]
fn discard_empties_the_queue() {
    if !speakers() {
        return;
    }
    let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 150).unwrap();
    pcm.pause(true);
    pcm.write(&silence(48_000, 50)).unwrap();
    pcm.discard();
    assert_eq!(pcm.delay(), 0);
}

#[test]
fn drain_waits_until_everything_is_heard() {
    if !speakers() {
        return;
    }
    let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 150).unwrap();
    pcm.write(&silence(48_000, 100)).unwrap();
    let started = std::time::Instant::now();
    pcm.drain();
    assert!(started.elapsed() >= std::time::Duration::from_millis(50), "{:?}", started.elapsed());
    assert_eq!(pcm.delay(), 0);
}
```

`engine.rs` tests (replaces the Stage 1 racing test):

```rust
#[cfg(target_os = "macos")]
#[test]
fn the_default_device_plays_a_track_at_its_own_speed() {
    if crate::output::available().is_err() {
        eprintln!("no sound output device: skipped");
        return;
    }
    let wav = silent_wav("macos-speed", 1000);
    let (tx, events) = crossbeam_channel::unbounded();
    let engine = Engine::spawn(crate::output::DEFAULT.into(), Arc::new(Sources::default()), move |e| {
        let _ = tx.send(e);
    });
    let started = std::time::Instant::now();
    engine.send(EngineCmd::Open { id: 1, location: wav.to_string_lossy().into(), start_ms: 0 });
    assert_eq!(events_until_finished(&events), ["started 1", "finished 1"]);
    let _ = std::fs::remove_file(wav);
    assert!(started.elapsed() >= Duration::from_millis(800), "{:?}", started.elapsed());
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p popamp-audio 2>&1 | grep -E "^error|FAILED" | head`
Expected: `ring_capacity` missing (compile error).

- [ ] **Step 3: Implement `open`**

- **`default`:** component `kAudioUnitType_Output` / `kAudioUnitSubType_DefaultOutput` / `kAudioUnitManufacturer_Apple`, with latency from `default_output()`.
- **A UID:** `kAudioUnitSubType_HALOutput`, then `kAudioOutputUnitProperty_CurrentDevice` (global scope) set to `device_by_uid`'s ID.
- **Then:**
  - Set `kAudioUnitProperty_StreamFormat` on the input scope, element 0, to linear PCM `kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked`, 32-bit, `channels` interleaved, at `rate`.
  - Set `kAudioUnitProperty_SetRenderCallback` (input scope) to `render`, with the boxed `Shared` as `inputProcRefCon`.
  - `AudioUnitInitialize`, then `AudioOutputUnitStart`.
- **Ring:** `rtrb::RingBuffer::new(ring_capacity(rate, channels, latency_ms, device_buffer))`.
- **Latency:** `latency_frames` = `to_stream_frames` of `device_latency`, plus `kAudioUnitProperty_Latency` (global, seconds) × `rate`.
- **Errors:** any non-zero status disposes what was made and returns `Err(format!("CoreAudio cannot play {rate} Hz / {channels} ch: error {status}"))`, in the ALSA message's style.
- **`ring_capacity`:** `max(rate * latency_ms / 1000, 2 * device_buffer_frames) * channels`.

- [ ] **Step 4: Implement the callback and the methods**

- **`render`:** `try_lock` the consumer; copy `min(available, requested)` samples into `mBuffers[0]`; fill the rest with `0.0`. If the lock is busy (a `discard` is under way), play silence. Return 0.
- **`write`:** push what fits through the producer's chunks. While anything is left, nap for a quarter of the device buffer's duration (clamped to 1–10 ms), then push again.
- **`delay`:** the queued frames (`(capacity - producer.slots()) / channels`), plus `latency_frames` when any are queued; 0 when nothing is.
- **`pause(on)`:** `AudioOutputUnitStop` or `AudioOutputUnitStart`, keeping the queue; returns `true`.
- **`discard`:** lock the consumer and skip everything in it.
- **`drain`:**
  - While playing: wait (napping as `write` does) until the queue is empty, then sleep for `latency_frames` at `rate`. Give up after the queue's own duration plus 1 s.
  - While paused: return at once.
- **`Drop`:** stop, uninitialize and dispose the unit, then free the `Shared` box. The order matters: the callback must be gone before its data is.
- **Thread safety:** `unsafe impl Send for Stream`, with a `SAFETY` comment saying the engine uses it from one thread at a time, as `AlsaPcm` does.
- **Remove** the Stage 1 "not yet" text and the module doc's "only `null` plays for now", here and in `output/mod.rs`.

- [ ] **Step 5: Run the tests and lints**

Run: `cargo test -p popamp-audio && cargo clippy -p popamp-audio --all-targets --locked -- -D warnings`
Expected: all pass, including `a_device_opens_by_its_uid` from Task 1. No warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/popamp-audio
git commit -m "Play sound on macOS through CoreAudio

An output AudioUnit per stream, the default one or one bound to the
chosen device, fed from a queue its render callback reads without
waiting, so the engine's blocking writes work as on ALSA and WASAPI."
```

---

### Task 3: The whole workspace, and hearing it on the Mac

**Files:** none new.

- [ ] **Step 1: All checks, all three feature sets**

Run: `cargo fmt --all -- --check && for f in "" "--features popamp/plugins" "--features popamp/plus"; do cargo clippy --workspace --all-targets --locked $f -- -D warnings && cargo test --workspace --locked $f || break; done`
Expected: clean and passing.

- [ ] **Step 2: Play a local file**

Run: `cargo run -p popamp --release -- /NEW /INIDIR=/tmp/popamp-dev <some .mp3 or .flac on the Mac>`
Expected: it plays at the right pitch and speed. Pause and resume work. Seeking while paused, then playing, gives no burst of old audio. The time display advances. Ask the contributor to listen. This is the first change they can hear.

- [ ] **Step 3: Radio and devices (with the contributor)**

- **Radio:** `cargo run -p popamp --release -- /NEW /INIDIR=/tmp/popamp-dev https://somafm.com/groovesalad.pls`
  - Expected: the stream plays and its title shows.
- **Device list:** open Preferences.
  - Expected: the Output device list shows "Default device" and the Mac's devices by name.
- **Switching while playing:** pick another device during playback.
  - Expected: playback moves there and goes on from the same place.
- **Switching back:** pick "Default device".
  - Expected: playback returns.

Record what was heard in the ledger.

- [ ] **Step 4: Push and open the stacked draft PR in the fork**

```bash
git push -u origin macos/2-sound
gh pr create -R miraclebg/popamp --base macos/1-build --head macos/2-sound --draft \
  --title "macOS: sound through CoreAudio (stage 2)" --body "Fork-internal; stacked on #1. Not for upstream yet."
```

CI runs Linux, Windows and MSRV on the PR (macOS runs only after merges). Expected: green.

---

### Task 4: Shaped-skin spike (throwaway)

A spike: no tests, not merged, not reviewed. It answers one question for the spec: can a `region.txt` skin be shaped on macOS through a transparent window?

**Files:** on a throwaway branch `spike/macos-shapes` from `macos/2-sound`; nothing from it is kept but the findings.

- [ ] **Step 1: Pick a shaped skin**

Find a classic `.wsz` with a `region.txt` through Popamp's Skin Museum browser, or ask the contributor for one.

- [ ] **Step 2: Try it**

In `platform/macos.rs` only:
- create windows with `with_transparent(true)`;
- in `set_shape`, keep the mask;
- make the pixels outside it fully transparent when presenting.

If `softbuffer` drops alpha on macOS, try presenting the frame yourself: a premultiplied-alpha `CGImage` set as the contents of the window view's layer. Check three things: clicks pass through the cut-out areas, the shadow follows the shape, and dragging still works.

- [ ] **Step 3: Write down what was learned**

On `macos-port-design`, add a "Spike result" paragraph to the spec's Shaped-skin spike section:
- whether it worked;
- whether `softbuffer` carries alpha;
- what the real change would touch;
- whether shaped skins move into Stage 3 (the spec's rule: yes if it worked).

Commit and push that branch. Delete `spike/macos-shapes` locally and leave it unpushed.
