# Slot. for KONKR — dev18 parity checkpoint

Reference date: 2026-10-10. This is an **Android 12 application**, not a
replacement OS. Original: `BrandonKowalski/slot`. Port: `Baggio94/slot-konkr`.

> **Verification rule:** a successful GitHub Actions build is not a hands-on
> test on the KONKR Pocket Advance. The last explicit handheld confirmation
> in the original conversations is dev11; dev12–dev18 require consolidated QA.

## Development chain (keep each predecessor)

| Branch | What was integrated |
| --- | --- |
| `feat/konkr-dev13-settings-about` | Original-style Settings and About; 2/3/4/6× Fast Forward; sound; rumble; colour toggle; dev13 values persist. |
| `feat/konkr-dev14-screen-gameplay` | Screen and Gameplay submenus, upstream shader names, GB palettes, X/Y turbo mapped through Slot's original button algorithm, Rewind toggle, Auto Save on Eject. |
| `feat/konkr-dev15-gb-display` | Original `video_mode.rs`: GB/GBC Actual and Stretch, per-cartridge `Config/video_mode.ini`; About uses Android `versionName` via `SLOT_KONKR_VERSION`. |
| `feat/konkr-dev16-personalization` | Original `Theme::read`, original Slot wallpaper compositor, Android SAF import/reset into private `Config/theme.txt` and `Wallpapers/user.png`. |
| `feat/konkr-dev17-cart-labels` | Import/remove selected game's PNG label by controller, `Labels/<stable-uri-key>.png`, Slot's original `art::cover` crop and cartridge renderer. No ROM or source artwork changes. |
| `feat/konkr-dev18-original-color` | Original mGBA/gpSP colour-correction core options replace dev13's approximate GPU grade. Original mGBA `mgba_gb_colors_preset` and model settings; 336-byte ROM header DMG guard. |

## Original five-axis plan

1. **BIOS / second gpSP**: already implemented; no reintegration planned.
2. **Saves and states**: already implemented, RetroArch SAF/RZIP, Polaroids, undo; full gpSP cross-app compatibility to confirm during QA.
3. **Emulation**: R2 FF, L2 rewind, X/Y turbo, configurable rewind, rumble, per-game GB/GBC aspect ratio. Hardware-only audio quality and vibration checks remain.
4. **Visual parity**: Shader selections, original slot chrome and cart animation, Game Boy palettes and libretro colour correction integrated. Pixel fidelity/GB palette behaviour on hardware not yet verified.
5. **Personalization**: Screen and Gameplay, About, original theme and wallpaper, per-game PNG cart labels. A separately approved Android adaptive icon ZIP still has to be supplied and integrated.

## Important compatibility invariants

- `fyi.slot.konkr`: never uninstall to update; `adb install -r`.
- Maintain persistent release signing key/keystore. Never attach keystore or secrets.
- No fake demo games, no BIOS/ROM distribution. SAF must not modify source ROMs.
- Preserve the original ~730 ms insertion-to-CRT timeline, including 450 ms insertion and 280 ms hold.
- Preserve direct SAF reads/fallback and the missing-RTC error fixes from dev6–dev8.
- `Settings::load` migrates dev13 JSON; previously tested FF defaults remain 2× and FF Sound ON, despite different default values in current upstream Slot.
- Distinguish different libretro cores; mGBA states are not gpSP states.
- No full RetroAchievements or scraping until Slot parity is checked, per original scope decision.
- Imported user wallpaper/theme/labels are **app-private**, not changes to source SAF files.
- Cart Studio on KONKR is **PNG label import**, not a claim that the upstream repository contains a full built-in image editor.

## Consolidated testing **later**, at user's request

1. Install a CI-verified, permanently signed APK in place and inspect actual build version in About.
2. Navigate all menus with physical controls, test B/A/caret changes and persistence after restart.
3. Compare each GBA/GB shader with original Slot; 48 DMG palettes; `mgba_color_correction` and `gpsp_color_correction` visibly on/off; GB/GB-compatible/GBC distinctions.
4. Check GB/GBC L1 Stretch and R1 Actual, per-game persistence, and SELECT+L1/R1 manual state shortcuts.
5. Test turbo X/Y, rewind toggles, auto-save disabled/enabled, Fast Forward speed/audio at 2/3/4/6×, rumble on real compatible cores.
6. Import and remove wallpaper, theme and cart PNG using SAF; visually inspect cartridge material and labels.
7. Validate gpSP and mGBA saves/states in both directions with RetroArch, using **copies** rather than valuable saves; RTC, missing-file and manual slot cases.
8. Check cold start, launch timing, normal ejection, Suspend/Resume, RAM/battery/sleep. No claim of a stable release before these tests.

## Still outstanding

- Integrate the exact approved `Slot-for-KPA-Android-Icon-Pack-v1` ZIP from the separate design discussion; the archive was not part of this transfer.
- Confirm pixel/colorimetric fidelity, ROM/BIOS corner cases, Android HAL/audio, and save state interoperability on actual hardware.
- After hardware validation and corrections: prepare RC/stable, update release notes. Full RetroAchievements and scraping are explicitly postponed.
