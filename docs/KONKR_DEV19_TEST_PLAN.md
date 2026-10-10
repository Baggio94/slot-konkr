# Slot. for KONKR — dev19 physical QA

Branch: `feat/konkr-dev19-original-gestures-crt-boot`
Version: `0.0.7-dev19`, code 28. Signed in CI with the stable certificate.

## Changes against dev18

- Replicate upstream v1.5.0 ejection order: retain last libretro frame, CRT-off 160 ms, black hold 350 ms, then cartridge retract 450 ms; start eject PCM as mechanical motion begins.
- SELECT is deferred for a short chord window (600 ms, as upstream). Short standalone taps are still delivered to libretro; consumed chords consume their releases.
- SELECT+A/B step the original GBA/GB/GBC shader lists with upstream HUD toasts.
- SELECT+L2/R2 step the 48 actual mGBA palette presets when palettes are enabled and the ROM is DMG-compatible; no rewind/FF overlap.
- SELECT+Y toggles the actual mGBA/gpSP libretro colour-correction option and shows upstream HUD toasts.
- SELECT+L1/R1 retain RetroArch-compatible load/save functionality.
- Valid DMG-only GB-compatible `.gbc` cartridges now qualify, matching upstream `core::palette_for`.
- Show unmodified upstream v1.5.0 `card/System/bootlogo.bmp` inside the Android Activity until the first rendered frame, fade 220 ms. Never alter the firmware or Android boot partition.

## QA on KONKR (not yet run)

1. Verify exact in-app boot logo and no stalled splash; first load with cached library and after refresh.
2. In GB (palette-enabled), SELECT+R2/L2 cycle palettes without FF/rewind; repeat on GB-compatible GBC ROM versus colour-only GBC.
3. SELECT+A/B cycle shaders and show matching toasts, SELECT+Y toggles colour correction, without simultaneously pressing in-game A/B/Y.
4. Verify bare SELECT is forwarded to game (short tap and long hold); SELECT+R1 save / SELECT+L1 load; confirm no input sticks after releasing SELECT first.
5. Compare CRT shutdown, 350 ms black delay and mechanical ejection PCM with stock Slot.
6. Regression: R2 hold + double tap FF latch; L2 rewind; MODE short/double/long; single-cart safeguards; SRAM/RTC/RetroArch state continuity.

## Still platform-specific / deferred

- Firmware volume/brightness/blue-light chording may need Android-native permissions and device key mapping; do not treat them as verified parity.
- Fast-forward audio ×1 (music composition at native tempo independent of game time) is not generally implementable through the libretro mGBA/gpSP API; the existing pitch-preserving accelerated audio and mute modes remain.
- Android icon asset pack was not provided to this development session; it remains a separate integration.
- Cores, saves, ROM directory grants, package ID, certificate secrets remain untouched.
