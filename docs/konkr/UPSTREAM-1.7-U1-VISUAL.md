# U1 — Official Slot v1.7 visual port (KONKR Pocket Advance)

> **QA candidate only.** This is not an approved stable release or a completed physical-device validation. Public `konkr-v0.1.0-rc.1` / Android code 90 is immutable. No code touching ROM contents, SRAM/RTC, RetroArch RZIP, Undo/Polaroids, multiplayer protocol, RetroAchievements or embedded Cart Studio is authorized in this U1 pass.

## Exact upstream reference

- `BrandonKowalski/slot` tag `v1.7.0`, immutable tree `34ac38031fd903eb2a0a0157f1dea4d3a5546195`.
- All **29 upstream PNGs copied byte-for-byte**, with matching git blob SHA-1; original source and license attribution retained.
- Source geometry is **720×480 (3:2)**. KONKR's physical framebuffer is **960×640 (3:2)**.
- Resampling is alpha-aware Mitchell cubic with premultiplied-alpha edges: cart GBA 480×283, GB/GBC 416×449, slot 960×77, wallpapers 960×640. Raw HD personal wallpapers decode directly at native resolution. **Gameplay/video pipeline unchanged**.

## Implemented U1 (CI gate is mandatory; manual validation still required)

1. Original slot rear/front skins in both Dark and Light, original neutral-plastic skins for GBA/GB/GBC, native label/circuit/PCB/front layers for transparent cart animations. The two background workers pre-render cart RGBA, with GPU work confined to GLES owner thread. The **42 texture** recycler persists.
2. `General`: `Wallpaper None / Random / Default / Default Blur / personal PNG`, no-repeat deck, conditional personal-only Tint `Off / Low / Medium / High` = `0/25/45/65%`, `Theme Dark/Light`, `Recents`, `Game Titles`, `Import Wallpaper`, `Delete Wallpaper`. Legacy `Wallpapers/user.png` is retained when a preference was not saved, while explicit `None` remains authoritative. `Config/theme.txt` remains supported.
3. `Recents`: fourth optional rack, ten persisted launch URIs, deduplication, removal filtering and correct real cart platform/core. The rack's GPU shadow, face assignments and selection survive list reorder; orphaned texture handles remain recyclable, stale worker output is discarded by per-rack render epochs.
4. `Game Titles`: native upstream `title_line_face` (32px), slot position and inverse 200ms / 1200ms / 800ms name fade. Titles **remain visible** at rest after 2200ms, exactly as original Slot, until the selected cart changes or the option is switched off.
5. `Screen`: independent shaders, colour correction, and bezels for GBA / GBC / GB. Real bundled GBC/GB original bezel overlays, no artificial forced GBA bezel on KONKR's 3:2 panel, per-ROM/core optional overrides and migration from old shared GB/GBC fields.
6. `Dark/Light`: both original slot faces cached up front. Shelf footer gauge, bolt, clock and battery percentage recolour according to upstream white (Dark) / black (Light); no GL allocation needed on toggle for these cached textures.
7. Menu structure and Pause rules are documented in `U1-PAUSE-EMULATION-MIGRATION.md` and are preserved by U1 visual work.

## Validation matrix (U1 acceptance)

- General: first-run empty rack; wallpapers None/default/blur/random/personal, tint visibility and persistence, import/delete, legacy settings migration.
- Recents: play 11 ROMs, 10 most recent unique; ON/OFF; missing/deleted ROMs; return from Recents to game and shelf; no stale cartridge face after reorder, no GPU leak.
- Screen: GBA/GB/GBC shader and correction independent; GB palette only where core supports it; GB/GBC bezel geometry without clipping; global vs ROM/core profile isolation.
- Appearance: compare Dark/Light slot, footer, gauge, clock, battery, prompts, labels, shadows, menus and title contrast with original Slot.
- Transparent skins: separate PCB/back/front through insertion/ejection/core-picker transitions; alpha halos; high-speed navigation; cache reuse after Recents rebuild.
- Pause: solo and Link/RFU; no States during Link; safe Resume, correct Back, persistent ROM/core preferences; Reset to Global and live pacing/Run-Ahead; no save mutations.
- Regression: Pokémon LeafGreen Flash SRAM/RTC, RetroArch states/RZIP, gpSP sound, mGBA continuity, RetroAchievements offline queue, X Cart Studio / SELECT core, X/Y Buttons Turbo unaffected.
- Performance: memory growth, worker queue stability, first texture readiness, 60fps pacing and audio during rapid scroll and multiplayer. Hardware Mac AVD + KONKR is still necessary.

## QA artifact, installation and release boundary

After all CI jobs succeed on the **exact final commit**, build `versionCode=93`, `versionName=0.1.0-rc.2-u1final-qa`. Artifact `slot-konkr-0.1.0-rc2-u1final-qa-signed-apk`, generated on the U1 feature branch, same application ID `fyi.slot.konkr`, permanent signature certificate SHA256 `4f42b8f0ac98f6a0102804ab51020ea0e7e2de7685c6ce09400a744de0b0fa2f`, both ARM64 and x86_64 JNI native libraries. **Never install a non-green or stale artifact.** Upgrade with `adb install -r` after signature validation, never uninstall or clear data. Mac AVD first, physical KONKR second.

**U2 only after Mac/KONKR acceptance**: favourites toggle on Y and favourite rack, other deliberately deferred upstream features. **Cart Studio modernization after U2**. No RC1 release tag/release edits.
