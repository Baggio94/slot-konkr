# Slot KONKR — Original Slot Feature Parity

**Priority decided 2026-10-10:** finish all original BrandonKowalski/slot features before our own enhancements.
RetroAchievements, RAOfflineProxy unlock integration and automatic artwork scraping are **PAUSED**.
The existing read-only RAOfflineProxy diagnostic stays intact and may be revisited later.

Sources: https://slot-cfw.fyi/ and https://github.com/BrandonKowalski/slot/blob/main/CHANGELOG.md
Source code is not equivalent to device validation; only call a feature complete after KONKR testing.

## Parity inventory

| Slot original | KONKR state | Acceptance requirement |
| --- | --- | --- |
| GBA/GB/GBC carousel, platform navigation, skip letters | GBA tested, GB/C need ROMs | Verify all consoles, shell proportions, animations |
| Mechanical insert/eject sounds, A short resume / A long fresh | Physically validated | Keep regression tests |
| SELECT original cart opening and chip animation | Physically validated | Add usable gpSP and persist per-game core |
| mGBA emulation and save RAM | GBA physically validated | Test GB/GBC core behavior |
| Original optional BIOS intro | 0.0.6-dev1 source, not yet tested | Load GBA/GB/GBC BIOS via Android SAF |
| gpSP second GBA core | 0.0.6-dev2 source + CI pending | Test physical gpSP boot, save RAM, audio and persistent choice |
| Hand-authored cartridge label PNGs and shell colors | Not implemented | Import Slot Cart Studio labels, respect cart shell colors, cache |
| Multiple timestamped save states | Only last automatic state | Complete state ring, thumbnails, save/load/delete and undo |
| Double MENU save-state switcher | Not implemented | A/B/X/Y switcher shortcuts and UI |
| SELECT + R1/L1 manual save/load | Not implemented | Input chords with no accidental gameplay presses |
| Fast-forward R2 / double R2, speed and sound | Not implemented | Configurable accurate fast-forward |
| Rewind L2 hold | Not implemented | Rewind state buffer with memory limit |
| Turbo X/Y for A/B | Not implemented | Frame-paced turbo behavior |
| GB/C L1 stretch and R1 native aspect | Not implemented | Preserve 4:3 versus stretch aspect |
| Shader presets LCD3x, Grid, Dot, Simpletex | Not implemented | Match original visual options by platform |
| SELECT + A/B shaders and Y color correction | Not implemented | Real GPU shaders and color correction |
| GB 48 palettes via SELECT + L2/R2 | Not implemented | Correct mGBA options and palette behavior |
| Screen/Gameplay settings, config persistence | Limited KONKR menu | Match original setting ranges, defaults, wrap |
| Brightness and blue-light shortcuts | Not implemented | Android window brightness and shader tint |
| Mute/volume, headphones, rumble | Game audio works, rest partial | Use Android-safe equivalents |
| Wallpaper, About, HUD/clock/power visuals | Partial | Match original graphics where applicable |
| Link Play GB/GBC/GBA across two devices | Not implemented | Emulated cable/wireless modes over Android network |
| SP lid close save/resume | Not implemented | Adapt using KONKR lid events without disrupting SleepManager |
| 3-minute lid power-off, system boot logo, power LED | Linux firmware-specific | Equivalent Android sleep/resume, app splash where possible; never modify Android bootloader |

## Execution sequence agreed

1. BIOS setup and game boot animation, then gpSP and true per-cart core selection.
2. Choose Save Folder and Choose Save State Folder with real SAF persistence; full original state ring/switcher/undo.
3. Original controller hotkeys and mechanics: FF, rewind, turbo, double MENU, aspect.
4. Original video shaders, palette selection, color correction and brightness/blue-light.
5. Original settings/quick menu, art import, per-cart shell colors, background caching, UI details.
6. Link Play and all hardware-dependent adaptations (lid, rumble, headphones, Android sleep/resume).
7. Only after user approval of original feature parity: resume RetroAchievements and automatic online scraping.

Important distinction: original Slot supports manually imported Cart Studio PNG labels; an automatic scraper
is a new enhancement and therefore comes later, as requested.

## BIOS implementation and device gate

Menu path: START > Menu > Library > Choose BIOS Folder (after Choose ROM Folder).
Recognize exactly gba_bios.bin (16,384 bytes), gbc_bios.bin (2,304 bytes) and gb_bios.bin (256 bytes).
Use Android ACTION_OPEN_DOCUMENT_TREE with persisted READ only; never move or modify source BIOS or ROMs.
Validate and stage selected BIOS in app-private filesDir/BIOS. Point libretro GET_SYSTEM_DIRECTORY there.
Set mGBA options mgba_use_bios=ON, mgba_skip_bios=OFF (actual upstream core options).
Game should show the original console boot intro when valid user-provided BIOS exists.
On next app open, reimport from persisted selection, serialized ahead of ROM staging.
Empty/invalid folder must not overwrite previous good BIOS selection.
Physical checks: UI width, selection and permissions, GBA logo, app restart persistence, GB/C separately
when ROMs available, sound/controllers/save states unaffected.

**States of completion:** source updated => CI green => user physically tests => mark accepted.
Do not claim 0.0.6-dev1 validated until the user actually tests on KONKR.


## gpSP second core — 0.0.6-dev2 integration gate

- Pinned source: libretro/gpsp at commit `5819380c2ffb0900219d700a382ee68c464ebb99`.
- `scripts/build-gpsp-android.sh` builds arm64 libretro core with Android NDK
  and packages `libgpsp_libretro.so` into the APK alongside mGBA.
- Slot original SELECT -> Left/Right chip hop -> A now persists *per opaque
  SAF ROM URI*; returning to SELECT shows the previously selected socket.
- Launch uses the core associated with this ROM; all other consoles GB/GBC
  continue to use mGBA. No extra Android emulator or file manager launches.
- gpSP uses the same private BIOS location as mGBA with its own libretro BIOS
  and boot-mode options. User-supplied BIOS from 0.0.6-dev1 is reused.
- Common cartridge SRAM is preserved, while emulator-specific savestates are
  kept separate as `.mgba.state` and `.gpsp.state` to avoid corrupt loads.
  Legacy `.state` files are still readable by mGBA.
- Physical validation: SELECT on GBA displays mGBA and gpSP chip/sockets;
  choose gpSP and confirm full cart closure, GBA boot, audio/D-pad, save/reopen,
  switch to mGBA and back, restart app to verify per-cart preference persisted.
- CI arm64 gpSP build and physical tests must complete before marking as done.
- RetroAchievements is frozen during this parity stage.
