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
