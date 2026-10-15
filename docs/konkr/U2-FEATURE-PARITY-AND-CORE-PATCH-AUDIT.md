# U2 — Slot v1.7 Android adaptation and core patch audit

Reference: BrandonKowalski/slot tag **v1.7.0** (2026-10-15).
U2 branch: `feat/konkr-upstream-1.7-u2` based on U1 head `986b456d`.
All work remains **unmerged, unpublished and not hardware-accepted** until QA.

## U2 scope and original Slot correspondence

- Shelf-only Y favorite toggle, yellow nine-pixel star from `slot-ui/icon.rs` at scale 2, one alphabetically ordered mixed GBA/GB/GBC FAVORITES rack; no toast, no per-ROM settings popup.
- Android-specific durable URI set `konkr-favorites.json`: separate from RetroArch SRAM/RTC/States and original card Config/favorites.txt, so no name clashes or ROM re-writes; missing ROMs are never presented.
- Stable graphics across Recents/Favorites reorders using native worker invalidation and 42-entry texture rekey. Unstarring the last favorite removes the rack; system rack navigation skips empties.
- Original v1.7 last shelf and cart selection, stored independently in `konkr-shelf-focus.json` with debounce and flushed on launch. Missing saved rack/URI falls back to actual platform and first playable entry.
- v1.7 LCD3x on **both GB and GBC**: `Shader::GB` includes Off, LCD3x, Grid and Simpletex, while GBA keeps Off/LCD3x/Grid/Dot.
- 400ms initial D-pad repeat in UI menus, then 110/85/65/50ms acceleration; never auto-repeat game inputs.
- Previously agreed U1 polish: exactly “Sign in securely. Achievements use Casual mode.” on one line, unified left inset for RetroAchievements login body, Settings order General/Screen/Gameplay/Emulation/Interface, submenu B returns to its original row.

## mGBA palette patch — investigation, NOT applied

Slot v1.7 carries `cores/mgba/gb-dmg-palette-follows-option.patch`:
- `GBVideoSetPalette` re-writes BGP/OBP0/OBP1 registers when DMG colors change.
- `GBVideoDeserialize` re-writes those registers after unserializing a state.

Our Android build script `scripts/build-mgba-android.sh` pins libretro/mgba commit
`7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6` and explicitly applies seven
patches (DMA, audio, Link and speculative-audio sidecar). It does **not** apply this
palette patch. Inspection of the pinned `src/gb/video.c` shows no equivalent BGP/
OBP register refresh in the two upstream locations. Therefore there is a concrete
source parity difference, rather than merely a missing filename.

**Decision:** do not silently change the pinned core in the same U2 UI build.
First test an isolated candidate by dry-running `git apply --check` against the
exact pinned revision, then rebuild for arm64 and x86_64 with the existing
speculative-audio ABI; ensure GB custom palettes work after saved-state restoration,
rewind and Run-Ahead. Keep existing ROM saves untouched and compare core behavior.

## gpSP JIT cache patch — investigation, NOT applied

Slot v1.7 carries `cores/gpsp/jit-cache-in-binary.patch` aimed at ARM64/Linux
boot conditions in which executable mmap of a JIT cache may fail. It removes
`MMAP_JIT_CACHE=1` for `platform=arm64` and places the translation cache in an
ELF `.jit` section in `arm64_stub.S`.

Our Android script `scripts/build-gpsp-android.sh` pins
`5819380c2ffb0900219d700a382ee68c464ebb99` (2026-09-19) and builds with
Android NDK, exporting sidecar symbols needed for safe speculative audio. The
pinned Makefile still sets `MMAP_JIT_CACHE=1` for ARM64.

**Decision:** not safe to copy the Linux device patch untested to Android; Android
executable mappings, SELinux policy, dynarec assembly and W^X constraints differ.
Audit actual Android crash traces and memory maps first. If reproducible, test a
separate signed arm64-only diagnostic core while retaining known-good binary and
save-state parity. No automatic gpSP patch or revision update in U2.

## Mandatory gates

- GitHub Actions on exact U2 head: Rust + Android tests, build, dual ABI, production
  signing identity and mGBA continuity.
- Mac AVD: settings, menu repeat/release, favorite label alignment, persistence,
  removal from Recents/Favorites, GB/GBC LCD3x, state load, keyboard mapping.
- Physical KONKR: 960x640 GL display, star on mixed shells, fast Y + L1/R1 use,
  memory/texture stability, flash/RTC, RetroAchievements, Link/RFU, RetroArch saves.
- Never uninstall or clear app data; never merge/publish/tag before acceptance.
