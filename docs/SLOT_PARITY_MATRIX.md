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
| Original optional BIOS intro | **GBA confirmed on physical KONKR in 0.0.6-dev1** | GB/GBC BIOS behavior still to check with games |
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


## gpSP integration outcome (0.0.6-dev2)

GitHub Actions run [38002922667](https://github.com/Baggio94/slot-konkr/actions/runs/38002922667) passed. The generated APK was checked for both ARM64 mGBA and gpSP libraries, original sounds, and gpSP exported libretro symbols. This establishes build integrity **only**, not playability on real hardware. Next device gate is selecting gpSP on a GBA cart, confirming launch/video/audio, returning to shelf, persistence of selected core and correct per-core save state isolation. Upstream BIOS folder 0.0.6-dev1 has been physically confirmed by the user to work; no regression testing of gpSP BIOS yet.


## Save parity development — 0.0.7-dev1 (NOT physically validated)

Experimental branch `feat/konkr-save-parity` extends the user-confirmed
0.0.6-dev2. Four explicit user UI decisions: **no synthetic demo carts on
first launch**, **no touchscreen interaction inside Slot**, **transparent top
text backdrop**, **no START+SELECT exit** (hold physical HOME/MENU instead).

Shared RetroArch folders: START > Menu > Library > Choose Save Folder / Choose
Save State Folder use persisted Android SAF READ+WRITE grants. The user should
choose the *root* RetroArch `saves` and `states` directories. Inside them
Slot uses the same per-core directory names `mGBA` or `gpSP`, then the
original ROM filename without extension, e.g. `mGBA/Advance Wars.srm` and
`mGBA/Advance Wars.state.auto`. These are distinct save formats:
- Save RAM `.srm`: same raw libretro battery RAM, imported before core boot;
  written back on successful suspend/eject. A `.before-slot` copy of any
  overwritten user file is retained, and write verified by reread.
- RetroArch state `.state.auto`: RetroArch 1 `RASTATE` block wrapper around
  core-specific serialized data. Existing `#RZIPv1#` deflate archives are
  decoded in a bounded Android worker; `#RZIPv2#` zstd is intentionally
  REFUSED/PRESERVED until an interoperable zstd decoder exists. A state produced
  by gpSP must not be loaded by mGBA or vice versa. Do not force-load an
  unrecognized or corrupted state. Export of valid uncompressed RetroArch
  state is available on exit, with backup before replacement.
- **RetroArch manual state slots (.state, .state1 etc.) are NOT yet imported
  into Slot's polaroid history or written by SELECT+R1.** This is a remaining
  integration task, not supported merely by choosing a shared folder. Original
  Slot local polaroids are independent from RetroArch files.
- Two applications should not write to the same save file simultaneously;
  concurrent RA changes need conflict-detection and recovery before full parity.

Original Slot save history: reuse `slot-store::StateRing` (10 stamped states
per ROM, per platform/core, evicts oldest only), original `thumb::png` capture
(240×160), `slot-ui::Polaroids` visual UI and dated/title captions, undo for
save/load for 30 seconds. SELECT+R1 creates a stamped history state; SELECT+L1
loads latest; short HOME opens game menu, double HOME opens Polaroids, B back,
A load, Y delete, X undo. Confirm these on real KONKR before declaring done.

Validation gates: Android+Rust CI (still in progress), then ROM/mGBA/gpSP
regression, initial empty shelf, transparence, controller-only input, exit
combo removed, long MENU exit, RA SAF folder access and permissions, SRAM
round trip RA→Slot→RA, state RASTATE round trip, v1 RZIP decode, v2 zstd
preservation, Slot history/undo. Do NOT distribute as stable until tested.

## Read-only actual KONKR RetroArch file audit — 2026-10-10

- RetroArch package: com.retroarch.aarch64.
- Audit enumerates /sdcard/RetroArch/saves and /sdcard/RetroArch/states,
  plus lowercase /sdcard/retroarch counterparts with apparently identical
  contents. Check whether both spellings identify the same Android SAF tree
  rather than assuming two separate stores.
- Save tree has mGBA and mgba spellings, with matching example files. Do
  not create duplicate folders; use the observed conventional mGBA name.
- mGBA .srm examples: 256 B (Mystic Quest GB), 32768 B (Metroid Fusion)
  and 131072 B (Pokemon GBA). A 48-byte Pokemon Silver .rtc exists: this
  RTC sidecar still needs explicit synchronization; save parity is incomplete
  for titles with an independent RTC file.
- mGBA automatic states end with .state.auto and start with hex
  23 52 5A 49 50 76 01 23 => #RZIPv1#.
- RZIP header indicates 131072-byte uncompressed chunk size, followed by
  64-bit total raw size and per-chunk compressed lengths. The examples
  show zlib data beginning 78 9c.
- RetroArchCompression.kt supports bounded v1 zlib decoding and, from
  commit f590e5f, encodes #RZIPv1# on state export. Existing .state.auto
  stays backed up before verified writeback. Zstd v2 is still unsupported.
- A listing and 32-byte headers are NOT enough to confirm actual game-state
  unserialization. Obtain one intact .state.auto sample for read-only
  decoding tests before asking the user to share real RetroArch folders
  with a development APK that can overwrite files.
- No gpSP saves/states are shown in this audit: nothing gpSP-specific can
  be claimed validated.
- ROMs/states created by different core versions can be incompatible even
  when filesystem and container formats match; recover and show errors.
- The intended no-folder fallback layout Saves/<core>/name.srm and
  States/<core>/name.state.auto is not yet fully implemented: current app
  private fallback still uses URI-digest filenames. Keep this as an open
  requirement; do not claim completed default-path parity.
