# macOS Stage 4: Something People Can Download

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A release builds `Popamp-<version>-macos-arm64.dmg` and `Popamp-Plus-<version>-macos-arm64.dmg` in CI: a real `Popamp.app` with its icon, carrying libopus, libopenmpt and the MIDI SoundFont, running on macOS 12 and newer. It is signed and notarised with a Developer ID when credentials are configured, and ad-hoc signed otherwise. The documentation says Popamp runs on macOS.

**Architecture:** The same shape as the Windows downloads.
- `packaging/macos/fetch-libs.sh` builds libopus and libopenmpt from pinned source releases for macOS 12. Homebrew's builds require macOS 26/27 (`minos 26.0` and `27.2`), so they can't be bundled.
- It also fetches the pinned GeneralUser GS SoundFont and their licences.
- `packaging/build-macos.sh` turns a release `popamp` into `Popamp.app` (or Plus), signs it, wraps it in a `.dmg`, and notarises it when it can.
- `release.yml` gets a `macos` job beside `linux` and `windows`.

The program itself gains one thing: it finds the SoundFont in the bundle's `Resources`.

**Tech Stack:** shell (POSIX `sh`, as the other packaging scripts), `install_name_tool`, `codesign`, `hdiutil`, `xcrun notarytool`/`stapler`, autotools for libopus and libopenmpt, GitHub Actions `macos-15`.

**Spec:** `docs/superpowers/specs/2026-11-12-macos-port-design.md` (branch `macos-port-design`): Stage 4, and the decisions on bundle identifier (`app.popamp.popamp`), signing (Nimasystems' Apple Developer account, free of charge) and CI.

## Global Constraints

- `LSMinimumSystemVersion` 12.0.
  - Every bundled binary (the program and each `.dylib`) must say `minos 12.0` (`otool -l … | grep minos`).
  - It may link only `/usr/lib` and `/System` libraries plus the other bundled ones, through `@executable_path/../Frameworks/…`.
  - `MACOSX_DEPLOYMENT_TARGET=12.0` for `cargo build` and the library builds.
- Apple Silicon (`arm64`) only, the contributor's choice. Scripts take `ARCH` (default `arm64`) so Intel and universal builds can come later.
- Pinned downloads with SHA-256 checks, as `packaging/windows/fetch-libs.sh` does: libopenmpt 0.8.10 (the Windows build's version), opus 1.5.2, and the GeneralUser GS commit and hash from the Windows script.
- Signing and notarisation are optional and driven by the environment. Nothing secret goes in the repository.
  - `MACOS_SIGN_IDENTITY` (a "Developer ID Application: …" name) signs with the hardened runtime and a timestamp.
  - `NOTARY_KEY_PATH`, `NOTARY_KEY_ID` and `NOTARY_ISSUER` (an App Store Connect API key) notarise the `.dmg` and staple it.
  - Without them the app is signed ad hoc and the script says so.
- Popamp Plus runs WebAssembly through wasmtime, which writes machine code at run time. Under the hardened runtime it needs the `com.apple.security.cs.allow-jit` entitlement; the player does not.
- Both editions are `Popamp.app` with bundle identifier `app.popamp.popamp`. Installing one replaces the other and they share settings, as on Linux and Windows.
- Branch `macos/4-release` from `macos/3-feel` (or from `main` once PR #5 is merged; same content). One commit per task, each with the `Claude-Session:` line. Nothing goes upstream.

## Review Focus

1. **A Mac on macOS 12 to 25.** Expected: the app opens, and Opus, tracker and MIDI files play. Pinned by the `minos` and `otool -L` checks in `build-macos.sh`'s self-check (Task 3).
2. **Double-clicking a downloaded, ad-hoc-signed `.dmg` app.** Expected: Gatekeeper blocks it, and the docs say exactly how to open it anyway. Pinned by the docs (Task 5) and a manual check (Task 6).
3. **Popamp Plus with plug-ins under the hardened runtime.** Expected: a plug-in still loads (the JIT entitlement). Pinned by `codesign -d --entitlements` in the self-check (Task 3) and a manual plug-in check (Task 6).
4. **MIDI out of the box.** Expected: a `.mid` plays with no SoundFont set, using the bundled one. Pinned by `the_soundfont_in_an_app_bundle_is_found` (Task 1).
5. **A release tag.** Expected: the GitHub release has both `.dmg` files and their checksums, next to the Linux and Windows files. Pinned by the release job's `needs` and a `workflow_dispatch` run (Task 4).

---

### Task 0: Branch

- [ ] `cd /Users/mkovachev/sworkspace/popamp-macos && git fetch origin && git switch -c macos/4-release origin/macos/3-feel`. Expected: `Switched to a new branch 'macos/4-release'`.

---

### Task 1: The SoundFont inside the bundle

**Files:** Modify `crates/popamp-audio/src/midi.rs` (`find_soundfont`, around line 44, and its tests).

**Interfaces:** `find_soundfont(setting, program_dir)` also tries `program_dir/../Resources/GeneralUser-GS.sf2` when `program_dir` ends in `Contents/MacOS`. It goes after the file beside the program (Windows) and before the system paths.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn the_soundfont_in_an_app_bundle_is_found() {
    let app = std::env::temp_dir().join(format!("popamp-sf-{}", std::process::id())).join("Popamp.app/Contents");
    std::fs::create_dir_all(app.join("MacOS")).unwrap();
    std::fs::create_dir_all(app.join("Resources")).unwrap();
    std::fs::write(app.join("Resources").join(BUNDLED_SOUNDFONT), b"x").unwrap();
    assert_eq!(find_soundfont("", Some(&app.join("MacOS"))), Ok(app.join("Resources").join(BUNDLED_SOUNDFONT)));
    let _ = std::fs::remove_dir_all(app.parent().unwrap().parent().unwrap());
}
```

- [ ] **Step 2:** Run it and confirm it fails.
- [ ] **Step 3: Implement.** Use the same bundle test as `dylib::macos_candidates`: the folder is named `MacOS` and its parent `Contents`. Fix the comment on `BUNDLED_SOUNDFONT` to say "beside popamp.exe on Windows, in Resources in Popamp.app".
- [ ] **Step 4:** Run the crate's tests and clippy. Expected: pass.
- [ ] **Step 5: Commit:** `Find the SoundFont that Popamp.app carries in its Resources`.

---

### Task 2: Libraries for macOS 12, and the icon

**Files:**
- Create: `packaging/macos/fetch-libs.sh`
- Modify: `assets/logo/make-logo.py` (add 512 and 1024 renders and an `.icns` writer); create `assets/logo/popamp.icns` and `assets/logo/png/popamp-512.png`, `popamp-1024.png`

**Interfaces:** `packaging/macos/fetch-libs.sh <dir>` leaves in `<dir>`:
- `libopus.0.dylib` and `libopenmpt.0.dylib`, each with install name `@executable_path/../Frameworks/<name>`;
- `GeneralUser-GS.sf2`;
- `licenses/{opus.txt, libopenmpt/…, GeneralUser-GS.txt}`.

- [ ] **Step 1: Write `fetch-libs.sh`.**
  - Same style as the Windows one: `set -eu`, a `fetch <url> <sha256> <file>` helper, a temporary folder removed on exit, and pinned version and hash variables at the top.
  - **opus 1.5.2** (`https://downloads.xiph.org/releases/opus/opus-1.5.2.tar.gz`): `./configure --disable-static --disable-doc --disable-extra-programs CFLAGS="-arch $ARCH -mmacosx-version-min=12.0"`, then `make`.
  - **libopenmpt 0.8.10** (`https://lib.openmpt.org/files/libopenmpt/src/libopenmpt-0.8.10+release.autotools.tar.gz`): `./configure --disable-static --disable-openmpt123 --disable-examples --disable-tests --disable-doxygen-doc --without-mpg123 --without-ogg --without-vorbis --without-vorbisfile --without-portaudio --without-portaudiocpp --without-sndfile --without-flac`, with the same `-arch` and `-mmacosx-version-min` in `CFLAGS` and `CXXFLAGS`. Without those decoders it links only the system's libz and libc++; MO3 files with compressed samples are the only loss.
  - Copy each `.dylib` out, set its install name with `install_name_tool -id`, and copy the licences.
  - **Compute the two source hashes** on first download and pin them in the script. Record them in the ledger.
- [ ] **Step 2: Check it.**
  - Run `sh packaging/macos/fetch-libs.sh /tmp/popamp-libs`.
  - For both dylibs, `otool -l` shows `minos 12.0`, and `otool -L` shows only the library itself (`@executable_path/../Frameworks/…`), `/usr/lib/libz.1.dylib`, `/usr/lib/libc++.1.dylib` and `/usr/lib/libSystem.B.dylib`.
- [ ] **Step 3: The icon.** `make-logo.py` also renders 512 and 1024 (with Inkscape, as it does now) and writes `popamp.icns` with a small `write_icns`, next to `write_ico`: the `icns` header plus PNG entries `ic07` (128), `ic08` (256), `ic09` (512), `ic10` (1024), `ic11` (32) and `ic12` (64). Commit the generated `.icns` and PNGs, as `popamp.ico` is. Check with `iconutil -c iconset assets/logo/popamp.icns -o /tmp/popamp.iconset`.
- [ ] **Step 4: Commit:** `Build the Mac's libraries for macOS 12, and give Popamp a Mac icon`.

---

### Task 3: `packaging/build-macos.sh`: the app and its disk image

**Files:**
- Create: `packaging/build-macos.sh`, `packaging/macos/plus.entitlements`
- Modify: `packaging/macos/Info.plist` (`CFBundleIconFile` `popamp`), `packaging/macos/build-dev-app.sh` (point to `build-macos.sh` for releases)

**Interfaces:** `packaging/build-macos.sh [binary] [output-dir] [libs-dir]` (defaults `target/release/popamp`, `dist`, `dist/libs`). It works out the edition from `<binary> /VERSION`, as `build-windows.sh` does, and writes `<output-dir>/Popamp[-Plus]-<version>-macos-<arch>.dmg`.

- [ ] **Step 1: Write the script.** It refuses, with a hint, when the binary or libs are missing. Then:
  1. **Lay out the bundle:**
     - `Popamp.app/Contents/{MacOS/popamp, Info.plist, Frameworks/*.dylib}`;
     - `Resources/{popamp.icns, GeneralUser-GS.sf2, licenses/…, LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY.md}`;
     - `Info.plist` from the template with the version substituted, checked with `plutil -lint`.
  2. **Self-check:**
     - every Mach-O in the bundle has `minos 12.0`, and `lipo -archs` gives `$ARCH`;
     - `otool -L` of each names nothing outside `/usr/lib`, `/System` or `@executable_path/../Frameworks`;
     - `Contents/MacOS/popamp /VERSION` runs.
     - Any failure stops the script.
  3. **Sign:**
     - **With `MACOS_SIGN_IDENTITY`:**
       - `codesign --force --timestamp --options runtime --sign "$MACOS_SIGN_IDENTITY"` each dylib first;
       - then the app, adding `--entitlements packaging/macos/plus.entitlements` when the edition is Plus;
       - then `codesign --verify --deep --strict`, and for Plus check `codesign -d --entitlements -` lists `allow-jit`.
     - **Without it:** `codesign --force --deep --sign -`, and print "ad hoc: Gatekeeper will ask; see docs/building.md".
  4. **Disk image:** `hdiutil create -volname "Popamp <version>" -fs HFS+ -format UDZO`, with the app and an `Applications` symlink, from a staging folder.
  5. **Notarise:** with the three `NOTARY_*` variables and a signed app, run `xcrun notarytool submit <dmg> --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER" --wait`, then `xcrun stapler staple <dmg>`. Otherwise skip, and say so.
- [ ] **Step 2:** `plus.entitlements` is a plist with `com.apple.security.cs.allow-jit` set to true; check it with `plutil -lint`.
- [ ] **Step 3: Check both editions locally.**
  - Build them with `MACOSX_DEPLOYMENT_TARGET=12.0 cargo build --release --locked -p popamp [--features plus]` and run the script after each.
  - Expected: two `.dmg` files. `hdiutil attach` shows `Popamp.app` and `Applications`, the self-check passes, and they are signed ad hoc.
- [ ] **Step 4: Commit:** `Package Popamp.app and its disk image for macOS releases`.

---

### Task 4: The release job

**Files:** Modify `.github/workflows/release.yml` (a `macos` job, and `release` needs it; the header comment names the Mac downloads and the optional signing secrets).

- [ ] **Step 1: Add the job.** Runner `macos-15`, timeout 60 minutes, steps:
  1. checkout, then the Rust toolchain;
  2. the same tag-versus-`Cargo.toml` check as the `linux` job;
  3. `brew install opus libopenmpt` (the tests load them; the bundle uses its own);
  4. `cargo test --release --locked --workspace`, and again `--features popamp/plus`;
  5. `sh packaging/macos/fetch-libs.sh dist/libs`;
  6. with `env: MACOSX_DEPLOYMENT_TARGET: "12.0"`, build and package each edition with `build-macos.sh`.
  - **Signing,** only when the secrets exist: an earlier step imports `MACOS_CERTIFICATE` (a base64 `.p12`) with `MACOS_CERTIFICATE_PASSWORD` into a temporary keychain, sets `MACOS_SIGN_IDENTITY`, and writes `NOTARY_KEY` (a base64 `.p8`) to a temporary file for `NOTARY_KEY_PATH`, with `NOTARY_KEY_ID` and `NOTARY_ISSUER`. Secrets go through `env:`, never interpolated into `run:` text.
  - **Upload:** `actions/upload-artifact` with `name: macos-arm64`, `path: dist/*.dmg`.
  - **Publish:** the `release` job's `needs` becomes `[linux, windows, macos]`.
- [ ] **Step 2: Check it.** Push the branch and run the workflow by hand: `gh workflow run release.yml -R miraclebg/popamp --ref macos/4-release`, then `gh run watch`. Expected: `macos` is green, its artifact holds both `.dmg` files, and `linux`/`windows` are green as before (the `release` job is skipped, since there's no tag).
- [ ] **Step 3: Commit** (before Step 2's push): `Build the macOS disk images in the release`.

---

### Task 5: Documentation

**Files:** Modify `README.md`, `docs/building.md`, `CONTRIBUTING.md`, `THIRD_PARTY.md`, `CHANGELOG.md`.

- [ ] **Step 1: Write it, in the project's plain voice.**
  - **README:**
    - the badge `Linux | Windows | macOS`;
    - "runs on Linux, Windows and macOS";
    - the install table gains `macOS (Apple Silicon) | Popamp-<version>-macos-arm64.dmg, or Popamp-Plus-…: open it and drag Popamp to Applications`;
    - while releases are ad-hoc signed, one line on opening it the first time.
  - **`docs/building.md`:**
    - a macOS section next to Windows: macOS 12+ on Apple Silicon, Rust and the Xcode Command Line Tools, nothing else;
    - `brew install opus libopenmpt` for running from source;
    - `packaging/build-macos.sh` after `packaging/macos/fetch-libs.sh`;
    - the signing variables;
    - settings in `~/Library/Application Support/popamp`;
    - "Visualizer plug-ins … Metal on macOS".
    - First opening of an unsigned build: Control-click the app, then Open, or System Settings → Privacy & Security → Open Anyway.
  - **`CONTRIBUTING.md`:** the code-layout table names CoreAudio output and the Mac modules (Now Playing, Finder, dialogs through osascript).
  - **`THIRD_PARTY.md`:** "Shipped with the macOS downloads": libopus (BSD), libopenmpt (BSD), GeneralUser GS (its licence), as the Windows table does.
  - **`CHANGELOG.md`:** an Unreleased entry, "Popamp runs on macOS (12 and newer, Apple Silicon)", with the headline features.
- [ ] **Step 2:** Re-read each file and check every path and command it names exists.
- [ ] **Step 3: Commit:** `Say how to get and build Popamp on macOS`.

---

### Task 6: By hand, and the draft PR

- [ ] **Step 1:** Run all checks, on all three feature sets. Expected: clean and passing.
- [ ] **Step 2: With the contributor:**
  - Open the locally built `.dmg` and drag Popamp to Applications, replacing the development copy. Start it from Launchpad.
  - Check that these play: a file, the SomaFM `.pls`, an `.opus`, a tracker module (`crates/popamp-audio/tests/fixtures/sine.mod`) and a `.mid` (`crates/popamp-audio/tests/fixtures/a4.mid`, with no SoundFont set).
  - Check the Dock and Finder show the Popamp icon.
  - With Plus: if any plug-in is installed, it loads.
  - Optionally, download the CI artifact's `.dmg` to see Gatekeeper's prompt and the documented way past it.
- [ ] **Step 3:** Push, then open the draft PR: `gh pr create -R miraclebg/popamp --base main --head macos/4-release --draft --title "macOS: downloadable Popamp.app and disk images (stage 4)"`, with the session line in the body. Expected: CI green.
- [ ] **Step 4: Tell the contributor** which repository secrets turn on signing and notarisation (`MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PASSWORD`, `NOTARY_KEY`, `NOTARY_KEY_ID`, `NOTARY_ISSUER`) and where they come from: a Developer ID Application certificate exported as `.p12`, and an App Store Connect API key. They add the secrets themselves; nothing secret passes through this session.
