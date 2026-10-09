# Slot KONKR — project status and integration roadmap

Last checked: 2026-10-09. Source of truth: GitHub branches and physical KONKR results, **not** declarations that a feature will work. Update this file as each test gate is completed.

## Product scope / immutable constraints

- KONKR Pocket Advance, Android 12, 960×640 landscape 3:2, arm64.
- Native port of BrandonKowalski/slot, preserve distinctive carousel/cartridge insertion/board animations and style.
- Android front-end with fully internal libretro emulation; **do not launch an external emulator**.
- Initial platforms GB, GBC and GBA. GBA first, optionally add more later through a new decision.
- Physical controller first; touch remains functional. Main ROM location is user-selected /ROMs using Android SAF, read-only; never move or remove user ROMs.
- Game save RAM/states in app-private storage; while previews use ephemeral debug signing keys, uninstalling the app erases them. Owner explicitly accepts loss of short test progress during prototyping: **do not interrupt every iteration by asking to back up a few seconds of test gameplay**. Before real user saves exist, establish a permanent signing key (secret, never public repo).
- Retain original Slot feel and hotkeys, with agreed KONKR overrides: START = library popup; SELECT on carousel = core / cartridge-board picker; top round BTN_MODE = logical Slot MENU (pending Android activity delivery test).

## Branches and physical status

| Branch | Status | Evidence |
| --- | --- | --- |
| `main` | Upstream/fork baseline; keep untouched | repository |
| `feat/android-bootstrap` | 0.0.3-dev1, confirmed **working on physical KONKR** | ROM scanning, mGBA GB/GBC/GBA game boot, audio/input, auto SRAM/state, controller exit |
| `feat/konkr-ui-feedback` | 0.0.4-dev1 source; **CI green, not yet tested on KONKR** | CI 37993694699; latest branch docs commit 25d98a2, no new code after green CI |
| `feat/konkr-retroachievements` | Discovery-only development; **CI green, not merged** | CI 37989369281; no actual unlocks |

Do not treat the independent RA branch and UI branch as merged. Consolidate into one integration branch after physically validating the new UI.

## What is already implemented

- `/ROMs` read-only SAF persistent folder permission, recursive GB/GBC/GBA discovery, 5000 cap, bounded GPU texture cache; native Slot-style cartoon cartridge faces.
- mGBA ARM64 libretro core bundled into the APK, via pinned upstream source. Core execution at proper FPS, audio with AudioTrack, gamepad/touch interaction.
- Atomic SRAM saved periodically and on exit; auto states saved on exit / pause, restored on normal launch.
- 0.0.4 UI branch: animate entire ~730 ms insertion before mGBA state restore; A tap resumes, hold >=400ms starts without auto-state; L1/R1 skip empty system shelves.
- 0.0.4 UI branch: START compact library menu (Choose ROM folder and Refresh library), SELECT cartridge-board display (mGBA installed, gpSP explicitly **not installed**), disabled Scrape labels option.
- Measured hardware: the round button beside L2 is Linux `BTN_MODE` on `/dev/input/event3` (`Microsoft X-box 360 pad`). Mapped tentatively to Android `KEYCODE_BUTTON_MODE` 110. In the 0.0.4 branch: short press carousel opens settings, short in-game opens pause menu, hold >=650ms saves/ejects. Android app-level delivery is **not confirmed**; vendor could intercept. START+SELECT fallback retained.
- RA-only branch: Android ContentProvider readonly RAOfflineProxy presence/status/actual port and pending awards; strict loopback transport routing model OFF/DIRECT/PROXY; no control side effects. Shelf X diagnostic. NO rcheevos evaluation or RA authentication.

## RetroAchievements decisions (binding architecture)

- Adapt the **rcheevos evaluation and memory bridging** design from pvaibhav/slot selectively, not a wholesale fork import.
- Softcore first. Hardcore after implementation correctness, mode restrictions, and appropriate RA requirements are validated.
- Emulation memory address translation is required per console and core; GBA integration first. **Do not assert pvaibhav already supports GB/GBC**. Current mGBA runs all three; Gambatte or alternate GB/GBC mapping only after separate verification.
- A protected RA account authentication/session and correct per-ROM game hash/ID, achievement definitions, memory callbacks each frame, and verified trigger/award submission are still missing.
- Route selection in Slot settings: OFF, DIRECT (official HTTPS), or RAOfflineProxy (localhost with status/provider-discovered port). Prefer proxy mode on this KONKR.
- **Online direct mode**: Slot talks directly to RA official HTTPS. **Online proxy mode**: Slot sends to localhost; RAOfflineProxy forwards to RA in real time and caches game data. Even when Wi-Fi is on, proxy mode is NOT a direct connection from Slot.
- **Offline proxy mode**: RAOfflineProxy's cached data enables Softcore awards; RAOfflineProxy is **sole queue owner** and syncs when online. Never simultaneously use pvaibhav's offline queue with RAOfflineProxy's queue. Never silently fall back to DIRECT when proxy not listening, because that bypasses cache/queue.
- Direct mode without Internet: do not claim success, attempt offline award queue or fallback until tested and explicit.
- Display clear states: earned locally / queued offline / confirmed by RA server. Ensure idempotency and app restart behavior; never mark RA confirmed merely from an HTTP queue acknowledgement.
- Slot only queries RAOfflineProxy, and does not start/stop the daemon behind SleepManager's back. SleepManager/BasicSync/RAOfflineProxy integration on the KONKR should remain intact, including OEM power management interactions such as DuraSpeed.
- No credentials/tokens in plain text, GitHub, logcat, or files exposed to other apps.

## UX, scraper, and core plans

- Use slot-cart-studio label generation concepts: original printed cartridge labels or stylized Studio logos/palettes, offline fallback filename label; match ROMs by platform and reliable hashes; cache assets locally. Prefer custom user art, never overwrite. ScreenScraper data and licensing/account limits require verification.
- Scraping work on a background worker; not inside the render frame. Consider existing ROMM scraped metadata as OPTIONAL future source, never require Raspberry Pi or a running ROMM instance.
- Core selection SELECT: original cartridge-board opening animation rather than Android settings popover. mGBA is installed; gpSP may be implemented later; do not label it playable early. If present, determine RA core compatibility before enabling achievements.
- MENU button short/long/double state machine, avoiding system HOME interception and overlapping key chords.
- Original Slot hotkeys backlog: double MENU => save-state switcher; SELECT+R1/L1 save/load; L2 rewind; R2 hold/toggle FF; X/Y turbo; SELECT+A/B shaders; SELECT+Y color correction; SELECT+L2/R2 palettes; L1/R1 aspect for GB/C; SELECT+MENU future link; display brightness/color temperature. **Documented, not implemented**.
- Keep touch support alongside controller support. Optimize menu visuals for 960×640 and avoid recreating many textures per UI frame.

## Execution gates and next actions

1. **UI device gate:** build/install 0.0.4, physically verify A animation and resume/fresh, L1/R1 GBA-only, START library selection/refresh, SELECT board, A/B and touch regression, live MENU short/hold. A CI green build alone is not sufficient.
2. **MENU event gate:** `adb logcat -s SlotKonkr:I` on device while pressing round key. Check `KONKR MENU BTN_MODE down/up`. If Android does not deliver, investigate Android keylayout/vendor mapping before changing shortcuts. If it works, implement reliable double-press and transition away from START+SELECT escape.
3. **Single integration branch:** merge/rebase the read-only RA discovery changes onto the validated UI branch. Resolve conflicts in MainActivity, Manifest, network XML, and CI deliberately. Run full CI + physical game regression; do not merge directly into main while features incomplete.
4. **RA stage A — safe transport and account:** explicit mode UI, proxy status/error, secure credentials, official account session, correct game hashing/ID and patch cache request (Softcore). Online traffic tests both DIRECT and PROXY.
5. **RA stage B — actual achievements:** memory read callback + correct GBA mapping, rcheevos client tick on emulated frames, notifications and persisted offline pending state only in proxy mode. Confirm one actual unlock online and offline sync and persistence across restart. Then GB/GBC mapping/core choice.
6. **Artwork stage:** Studio-style local label composition + custom PNGs, matching/scraping/cache and progressive loading; no accidental slowdown.
7. **Hotkeys + quality:** save-state UI, shaders/turbo/FF/rewind, core picker polish, pause/suspend/focus, performance/thermals, screenshot tests, signature permanence before nontrivial real data.

### Regression matrix (to run after every integration or engine change)

- Shelf and SAF permission persisted after relaunch, partial folders and empty systems, A/B/START/SELECT and touchscreen.
- mGBA boot GB/GBC/GBA (GBA confirmed on KONKR, GB/GBC to be physically tested), audio, controllable speed, 60 Hz, back/exit, pause/suspend, SRAM and auto-state.
- Async ROM cancellations: late worker success/error must not start or dismiss a different cart.
- MENU input short/hold/double vs OS HOME and gameplay chords.
- RA tests: no credentials in logs, direct online, proxy online, proxy offline cached award, queue replay once, offline reboot, refused/error case, queued/unconfirmed status, no dupe, no RA fallback.
- Dependencies coexisting on KONKR: SleepManager, RAOfflineProxy, BasicSync; preserve selected power and Wi-Fi behavior.

## Next physical test

Use the 0.0.4-dev1 APK from a green Actions artifact on UI branch. Preview GitHub debug signing may require uninstalling the previous app; the owner explicitly considers existing seconds of gameplay disposable for now. Then capture Logcat while pressing physical MENU to confirm Android-level delivery. Do NOT claim physical success until reported.

## Original Slot interface sounds — integrated, physical test pending

The original Slot repository ships exactly two UI PCM assets:
- crates/slot/assets/insert.pcm (240 ms; mono 16-bit LE at 48 kHz)
- crates/slot/assets/eject.pcm (315 ms; mono 16-bit LE at 48 kHz)

Upstream audio/sfx.rs plays those cart sounds, but no independent carousel
navigation click library is shipped. No extra UI beeps are being invented.
The earlier Android port only started AudioTrack while mGBA was playing,
which explains the absence of interface sound.

The experimental UI branch now packages both original PCM files in the APK.
CartSounds.kt adds WAV containers in cache and preloads both recordings with
Android SoundPool, independently of game audio. Rust emits insert/eject cues
at corresponding points in the cart animation; Kotlin polls on the GL render
thread rather than through the slower UI action timer.

Acceptance: CI build and APK asset presence; physical KONKR sound during
insert, game eject, cancelled insert, long MENU, repeated A/B, Android
pause/resume, proper volume and no duplicate cues. Not yet physically tested.
