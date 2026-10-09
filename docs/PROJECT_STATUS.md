# Slot KONKR — project status and integration roadmap

> **CURRENT PRIORITY (2026-10-10): Original Slot parity, NOT RetroAchievements.**
> The user has explicitly paused RA and automatic scraping until we reproduce
> the original Slot features. Work from branch feat/konkr-slot-parity.
> See [the parity matrix](SLOT_PARITY_MATRIX.md). Existing RA discovery
> remains read-only and unchanged.


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


## 0.0.4-dev2 — revised user-approved menu copy and navigation

- Only ONE onboarding sentence, displayed at TOP on first open:
  `Press START to open the menu and add your ROMs.`
- START on carousel opens MENU, not LIBRARY. HOME/BTN_MODE does NOTHING
  on carousel and remains reserved for game pause/exit.
- MENU items: Library, Scraping, RetroAchievements. B navigates back.
- LIBRARY submenu: Choose ROM Folder, Choose Save Folder,
  Choose Save State Folder, Refresh Library.
- Choose ROM Folder and Refresh Library work. Save and Save State folders are
  visible but disabled with a 'Save locations coming soon' note until the
  emulator's actual read/write pipeline uses the user-selected SAF folders.
  Never pretend picker selection changes actual saves before wiring access.
- Scraping and RetroAchievements submenus are placeholders that clearly
  say integration is in progress, not false claims of functional scraping/RA.
- Replaced oversized overlays with constrained 720x480 logical content,
  compact modal windows and fitted typography to prevent screen clipping.
- Retimed original 240ms insertion PCM to start ~480ms into animation so it
  ends at ~720ms, near the visual 730ms completion; eject PCM unchanged.
- SELECT exact original cart-board opening animation remains a separate task,
  not yet claimed complete. Original sound and menu require physical validation.


## Confirmed integration checkpoint

The 0.0.4-dev2 user interface, input and original cart sound were physically accepted on the KONKR. Integration branch `feat/konkr-integration` was created from that exact UI HEAD. RetroAchievements discovery and network configuration have been merged without replacing the newer UI MainActivity. Read-only RAOfflineProxy diagnostics are found under START > Menu > RetroAchievements, not on shelf X. This is NOT achievement evaluation, login or offline unlocking. Next tasks: reproduce exact upstream Slot `CorePicker` animation with original Rust timing, then validate merged app and implement rcheevos GBA + RAOfflineProxy transport in successive controlled milestones.


## Integration 0.0.5-dev1: SELECT original mechanical core picker

The integration branch now directly compiles the upstream Slot
`crates/slot/src/core_picker.rs` source via a relative Rust module path;
no reimplementation or time approximations for the selector's state machine:
slide 160 ms + lift 260 ms (open 420 ms), chip hop 180 ms, reverse close
320 ms, refusal shake and original chip physics. The rendering uses the
unchanged `slot-ui::board_from`, `lid_from`, `on_board`, socket and chip
rasterizers, `Draw::Turned` with the shared GPU backend. SELECT opens it
only for GBA, LEFT/RIGHT moves between sockets, A confirms, B reverses
and closes. Since mGBA is the *only installed core*, A on gpSP is refused:
the gpSP slot is drawn faithfully but not persisted or run until installed.
The original Slot legend has been ported with small text on the 720x480
logical canvas. Native physical tests and frame-by-frame visual comparison
are still required before saying the rendering is pixel-identical.

The RA-only branch has been merged into this integration branch with a
read-only RAOfflineProxy status check reachable from the nested Menu >
RetroAchievements section. The actual RA hash, account, rcheevos memory
callbacks, Softcore unlocks and offline forwarding are NEXT, not done.


## KONKR physical acceptance — 0.0.5-dev1

User physically confirmed 0.0.5-dev1: game works, full original cartridge
core picker opening/closing and chip swapping animations work, and the
read-only RAOfflineProxy status diagnostic works (localhost port 8080, online=true).
However, the picker footer had three text labels rendered from oversized
quick-menu font bitmaps; their x positions overlapped severely at 720x480
logical resolution. Fixed in 0.0.5-dev2 by using upstream slot-ui::hint_face
and arrows_hint_face to draw the original white keycaps plus properly sized
legends (B Cancel, left/right Swap, A Choose). Layout calculation constrains
spacing across widths; geometry and timing of CorePicker remain unchanged.

The status toast previously showed pending=? because pendingAwards.count was
missing in the queried provider JSON. Changed to `Pending awards: not reported`.
This is not the same as zero pending awards; the proxy is otherwise reachable
and online. RetroAchievements evaluation/unlocking remains unimplemented.
Physical 0.0.5-dev2 display acceptance remains pending.


## 0.0.6-dev1 BIOS parity checkpoint

The LIBRARY menu now includes Choose BIOS Folder between ROM and Save folders.
Android SAF imports only recognized, size-checked user-owned GB/GBC/GBA boot
ROMs to the app-private libretro system directory, not into /ROMs. The mGBA
BIOS loading options are enabled for a real boot intro. Other original Slot
features are tracked in [SLOT_PARITY_MATRIX.md](SLOT_PARITY_MATRIX.md).
RA/rcheevos work is paused at user request. This build is **not physically
validated** until tested on KONKR.


## 0.0.6-dev2 — real gpSP second core (physical verification pending)

The user confirmed the 0.0.6-dev1 BIOS folder and original GBA boot intro
work on KONKR. The next iteration adds real ARM64 gpSP to the APK, per-ROM
core selection in the original chip picker, and separate mGBA/gpSP
savestates. CI and on-device tests are required; do not claim gpSP playable
until tested on the KONKR. RetroAchievements remains paused.


## Active Save Stage — 0.0.7-dev1

The user accepted gpSP physically and requested RetroArch shared per-core saves
and states, original Slot's 10-state Polaroids thumbnails/undo, and removed all
initial demo carts, touch interaction, mismatched top banner and START+SELECT.
All four UX corrections were committed in branch
`feat/konkr-save-parity`. Rust and Kotlin wiring for explicit Save and State
SAF directories, RASTATE v1 wrapper, RZIP v1 decode, original Slot StateRing
and Polaroids are in source and still require CI and physical acceptance.
Current RetroArch state sync is .state.auto only; sharing manual numbered slots
is a known remaining feature. RetroAchievements remains paused.
