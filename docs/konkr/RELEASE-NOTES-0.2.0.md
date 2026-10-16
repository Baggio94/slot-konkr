# slot. for KONKR Pocket Advance — 0.2.0

**Public Android release** · Signed universal APK · **ARM64 (KONKR)** + **x86_64 (Mac Android emulator)** · Package `fyi.slot.konkr`.

## What's new

- **Faster cartridge loading and smoother navigation.** Up to 160 warm GPU cartridge faces (previously 96); a persistent LZ4-compressed artwork cache with a 160 MiB storage limit; two background raster workers; higher-priority visible-cart loading; idle-time preloading across GBA/GB/GBC and Recents/Favorites; and a GPU upload budget while scrolling. Original Slot's carousel physics and animation timing remain unchanged.
- **Ten GBA shaders plus Off.** LCD3x, LCD Grid V2, LCD Grid V2 + GBA Color, zfast LCD, LCD1x, Simpletex + GBA Color, AGB-001, AGS-001, Sharp Bilinear and Dot. Dot's RetroArch defaults are restored; LCD3x retains the original effect. Shader switches do not restart emulation.
- **Slot styling and usability.** Complete system labels (GAME BOY ADVANCE, GAME BOY, GAME BOY COLOR, Recents, Favorites), optimized cart artwork scheduling and original-style RetroAchievements notifications.
- **Per-game settings and migration.** GBA's previous Grid choice is migrated to LCD Grid V2; GB/GBC options are preserved. Existing per-game choices and saves are not wiped.

## Features retained

GBA, GB and GBC cartridge browsing; mGBA and gpSP; full recursive ROM folder indexing; Cart Studio and cartridge artwork; controller navigation and optional HOME launcher; rewind, fast-forward and guarded Run-Ahead; SRAM/RTC, savestates and Undo/Polaroids; RetroArch save import/export (including RZIP); RetroAchievements online/offline; Apps integrations; supported Link Cable and Wireless/RFU Host/Join multiplayer.

## Installation

Get `slot-konkr-0.2.0-arm64-x86_64.apk` below. **Update the existing installation**, do not uninstall or clear data:

```bash
adb install -r slot-konkr-0.2.0-arm64-x86_64.apk
```

The Android version is **0.2.0** (`versionCode 103`). It uses the same app ID and signing certificate as the previous public RC and QA builds. Verify the accompanying `SHA256SUMS.txt` if desired.

## Notes and limitations

- Shaders that are multipass in RetroArch have been adapted to Slot's Android GLES2 renderer. They are **not claimed pixel-identical**; image parity and frame pacing should be judged on actual hardware.
- The persistent artwork cache is derived data only; it may need warming on first startup, and malformed cache entries are regenerated. Cache files live under the app's private storage and are bounded.
- Multiplayer mode compatibility depends on the game/core; trusted local networks only. Peer identity checks are not transport encryption.
- **No ROMs, BIOS, copyrighted commercial game content, RetroAchievements credentials or personal saves are included.**

## Credits and source

Android adaptation of [Slot by Brandon Kowalski](https://github.com/BrandonKowalski/slot), with work building on [pvaibhav's fork](https://github.com/pvaibhav/slot). Original emulation core and shader authors are credited in the source, bundled licenses and build scripts. The GitHub release tag `konkr-v0.2.0` links to the exact corresponding source tree.
