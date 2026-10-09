# Slot for KONKR Pocket Advance — Android port

Upstream: https://github.com/BrandonKowalski/slot
Working branch: feat/android-bootstrap
Target: KONKR Pocket Advance, Android 12 (arm64), 960 x 640 (3:2).

## Goals
- Preserve original Slot GB, GBC, GBA shelves, cartridge graphics and insertion animations.
- Use original Rust UI/rendering crates; an Android fullscreen host owns EGL and physical input.
- Native integrated libretro emulation, starting with mGBA; proper game resume and SRAM/state persistence.
- Add RetroAchievements through rcheevos/rc_client and verified game hashes and memory maps, Softcore first.
- Navigate on hardware D-pad/buttons; touch is secondary; keep Android in charge of sleep/power/volume.
- Use existing legally obtained ROMs without moving them (Storage Access Framework with persisted permissions).
- Preserve upstream GPL-3.0-or-later, source, credits and third-party notices.

## Milestones and acceptance tests

M0: Fork and separate Android development branch; document architecture, probe device.
M1: Debug APK shows original Slot graphics (synthetic, non-playable test carts), fills 3:2 screen and navigates with D-pad.
M2: Device keycodes, touch, pause/resume, screen off/on, 60Hz pacing, diagnostics and battery checked on actual KONKR.
M3: SAF import, ROM metadata/labels, cart loading without copying or breaking ES-DE ROM paths.
M4: Android libretro host, in-game audio/video, SRAM, autosave, visual save states, return to carousel.
M5: Official RetroAchievements login, hash resolution, memory callbacks, rc_client per emulated frame, unlock popup, secure tokens.
M6: Stable alpha, release APK, native resolution typography, battery and regression tests.

## Constraints and risks
- Original Slot renders in a logical 720 x 480 coordinate space; 960 x 640 output means a fractional 4/3 scale. Fullscreen is not automatically native-pixel-quality; validate output and optimize text/assets.
- Linux framebuffer/evdev/ALSA/sysfs and poweroff must not be called in Android.
- Java/Kotlin host receives KeyEvent and MotionEvent; GLSurfaceView owns EGL context lifecycle.
- rcheevos is not only API calls. It needs accurate emulation memory access and authenticated client support.
- RetroAchievements Hardcore must be off until restriction rules and client approval are validated.
- A live APK and actual tests are NOT implied by this plan.
- Initial demo carts must not masquerade as playable games.
- Confirm hardware resolution, orientation, model, Android version, keycode and GPU driver using ADB.

## Local read-only diagnostics

    bash scripts/konkr-probe.sh

Follow with interactive key event identification:

    adb shell getevent -l

## Reference
Official website: https://slot-cfw.fyi/#hero
Original author: Brandon Kowalski
Video reference supplied by project owner: https://www.youtube.com/watch?v=vndbnpH0QoE
RetroAchievements implementation reference: https://github.com/RetroAchievements/rcheevos


## KONKR ROM library integration — 0.0.2-dev1

This build introduces read-only Storage Access Framework folder selection:
- Press START to choose the /ROMs directory or a parent of GB/GBC/GBA folders.
- Press SELECT to rescan the current folder; START selects a different folder.
- Android retains a persistent read-only grant. No root access, broad storage grant or ROM duplication.
- Scanning runs on a background executor with a maximum of 5,000 ROMs and bounded traversal.
- Supports raw .gba, .gb and .gbc. ZIP/7z and artwork scraping are not supported in this milestone.
- Original Slot renders generated label placeholders from ROM file names; artwork is later.
- Empty folders show empty shelves, not synthetic display cartridges.
- Cartridges are rasterized only when visible using a bounded pool of 42 GPU textures.
- SAF content URIs are opaque; Rust cannot read them through normal filesystem APIs.

The build still DOES NOT emulate or play games. Press A to see cartridge insertion.

RetroAchievements architecture (not yet implemented):
- Direct mode: native RetroAchievements HTTPS transport, optional local queue only after validation.
- RAOfflineProxy mode: localhost HTTP transport and proxy as the SOLE owner of offline awards, no double queuing.
- Proxy remains the HTTP endpoint while online to prepare its offline cache.
- Login, transport selection, unlocks, and emulator RAM integration will come in later releases.

Physical tests: choose ROM folder via START, verify GB/GBC/GBA shelves with L1/R1, A/B insertion,
relaunch for persisted access, SELECT rescan, empty folder handling, and no crash.

## Debug APK signing

GitHub-hosted builds currently use a fresh automatically generated Android debug signing key.
**0.0.1-dev and 0.0.2-dev1 have different signing certificates**, so Android cannot apply
0.0.2-dev1 as an in-place update of the earlier preview. Because 0.0.1-dev only contained
synthetic cartridges and no user library, uninstall its package before installing 0.0.2-dev1:

    adb -s BW0308N250009576 uninstall fyi.slot.konkr

This clears only Slot KONKR's own preview preferences, not the user's ROMs, ES-DE, RetroArch
or other app data. This is a one-time preview workaround, NOT an acceptable release update policy.

**Before 0.0.2-dev1 is used to store important data**, establish a persistent signing
keystore protected in GitHub Actions secrets or through a controlled local release process.
Do not commit private signing keys, release passwords or tokens to this public repository.
Treat unsigned/differently signed CI builds as non-upgradable until that is resolved.


## First playable Android milestone — 0.0.3-dev1

A reproducible Android arm64 mGBA libretro binary is built from a pinned upstream
libretro/mgba revision by scripts/build-mgba-android.sh. License notices ship in APK assets.
Android exposes the SAF ROM via a bounded copy into its private cache with the original
extension, never writing or moving the original. Rust then loads the cache through the
existing slot-retro libretro host. No external emulator app is launched.

- GB/GBC/GBA: core execution, controller mapping and realtime audio through Android AudioTrack.
- SRAM persisted as atomic saves every 30 seconds and on ejection/pause.
- Automatic resume state serialized on exit and pause; failure to restore does not delete files.
- Original shelf A inserts and starts selected ROM when staged; Android BACK returns to shelf.
- On shelf START opens SAF folder picker, SELECT refreshes; in game START/SELECT reach emulator.
- Game video uses original Slot graphics pipeline with an Android-safe RGBA conversion (GLES2).
- RetroAchievements / RAOfflineProxy are NOT yet implemented in this stage.

This is a developer preview, not a stable emulation release. Physical validation on KONKR
is still required for input, audio, 3:2 output, save recovery, sleep/resume and battery drain.
No commercial ROMs, BIOS files or mGBA core binaries are checked into Git.


Controller-only exit: press **START + SELECT** together during gameplay to save
and return to the original Slot cart shelf. Android BACK also exits. Touchscreen
taps hold game input across multiple emulated frames instead of getting lost in a
single JNI queue flush.


## UI feedback — isolated branch feat/konkr-ui-feedback

1. Insert animation completes before emulation core is opened and the previous
save state is deserialized. SAF ROM staging continues concurrently; once the full
~730 ms original animation is rendered, start the prepared ROM on the GL thread.
Cancellation by B discards prepared filenames, and late SAF work is matched against
the requested URI to prevent launch of a previously canceled game.
2. L1/R1 skip empty platform shelves; with only GBA ROMs both keys stay on GBA.
No additional empty-screen fallback UI is introduced by these fixes.

No changes to the validated feat/android-bootstrap branch or the experimental
feat/konkr-retroachievements branch. Remaining UI work: native Menu/Home keycode,
Start settings menu, Select cart core picker, original hotkeys, Studio-style labels.


## Integration from RA branch (October 2026)

The `feat/konkr-integration` branch contains all the validated 0.0.4-dev2 UI work PLUS the READ-ONLY RAOfflineProxy Android ContentProvider discovery, local-only networking security config and backup script from `feat/konkr-retroachievements`. The original branch remains unchanged.

Navigate START > Menu > RetroAchievements > A to check proxy status. It does NOT authenticate to RA, award achievements, change proxy configuration, or select a transport yet. The app will later support OFF, DIRECT and PROXY modes. In PROXY mode, RAOfflineProxy will be the sole owner of queued offline Softcore awards. No direct fallback if local proxy is unavailable.

The `X` diagnostic from the original RA exploratory branch is intentionally moved into the RetroAchievements submenu to avoid conflicting with controller conventions. Upstream `core_picker.rs` animation and frame-based rcheevos evaluation are separate follow-on milestones.


## Slot original parity — BIOS selection (0.0.6-dev1)

START > Menu > Library > Choose BIOS Folder opens an Android SAF directory
picker. We persist the read permission, validate files, and stage up to three
user-owned boot ROMs under private filesDir/BIOS: gba_bios.bin (16384 bytes,
first byte 0x18), gbc_bios.bin (2304 bytes), gb_bios.bin (256 bytes).
Only the selected read-only SAF folder is accessed, never mutated; no BIOS
binary is bundled in the APK. mGBA now sees that private directory as the
libretro system folder and uses mgba_use_bios=ON, mgba_skip_bios=OFF.
Import runs on the same worker as ROM copy, so a restart must stage BIOS
before first core load. Valid BIOS apply on the next game boot.
Save and Save State folder choosers remain disabled until their underlying
Android file writes are fully implemented. RetroAchievements is on hold.


## 0.0.6-dev2 — gpSP libretro

The bundled mGBA and gpSP cores run through the same Rust libretro runtime,
with Android GLSurfaceView rendering and AudioTrack. The selected core for
each SAF ROM URI is atomically stored in `Config/selected_core.ini` via a
stable hash, not ROM title (duplicates across folders are allowed).
SELECT opens the original CorePicker to the saved choice; LEFT/RIGHT swap
chips and A stores the preference. The selected libretro .so is loaded on the
next launch. BIOS import is shared, but save states use core-specific file
names. No RetroAchievements integration advances in this build.


## Save Stage 0.0.7-dev1 (experimental)

Android SAF read/write folder pickers select existing RetroArch SAVES and STATES
root dirs. Slot uses per-core mGBA and gpSP folders with exact ROM stems.
Existing saves read before game boot; exit/suspend queue a background flush
and verify external output. External original file is backed up as
`.before-slot` before first overwrite. ROMs and BIOS folders remain read-only.
Original Slot StateRing, PNG thumbnail and Polaroids UI are built in but not
validated. RASTATE v1 uncompressed and RZIP v1 deflate decoder are supported,
RZIP v2 zstd remains protected as unsupported, manual numbered RetroArch state
slots are future work. Do not treat a green CI as proof of round-trip safety.
