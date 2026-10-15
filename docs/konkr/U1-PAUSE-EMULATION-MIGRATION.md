# Slot KONKR U1 — Pause/settings emulation migration (implemented; QA pending)

This document supersedes the early code92 partial-pause notes. It describes the **current code**, not a future TODO. The current U1 final-QA candidate is code 93, and it must have a fully green CI on its own commit before user installation.

## Navigation and scope

- Carousel START → `Library / Cart Studio / RetroAchievements / Apps / Settings / About`.
- **Global** Settings → `General / Screen / Gameplay / Emulation / Interface`; only global Settings updates `slot-konkr-settings.json`.
- **Solo Pause** → `Resume / States / Multiplayer / Settings / Save & Eject`; **Linked Pause** → `Resume / Multiplayer / Settings / Save & Eject` (no States).
- Pause Settings → `Screen / Gameplay / Emulation / Reset to Global`, all changes implicit to the **current ROM + current core** without any “This Game” badge. Linked sessions show only safe Screen settings, disable state/save/speed/Run-Ahead/rewind/turbo changes, and keep Link/RFU connected when opening/closing its menu.
- Screen profile: GBA/GBC/GB Shader/Colour Correction/Bezel independently, GB palette only if supported; no artificial forced bezel for 3:2 GBA.
- Gameplay: Fast Forward speed/sound, Rewind, `X/Y Buttons`, Rumble, Auto Save on Eject.
- Emulation: Show Stats, Frame Pacing Standard/Responsive/Display Sync, Run-Ahead 0/1/2 subject to core/game safety.

## Persistence and safe live changes

- Per-game and per-core preferences use optional fields in existing `GameEmulation/<rom>.<core>.json`; legacy files deserialize normally. Global settings do not change when editing Pause.
- Screen shader and colour correction apply to the live session; pacing/Run-Ahead are read by the live engine without reopening the core. Time debt, speculative audio and Fast Forward state are cleared when appropriate. Pokémon mGBA Flash protection and gpSP speculative-audio safeguards stay enabled.
- `Reset to Global` removes only the selected ROM/core overrides and reapplies inherited values immediately. No other ROM/core's settings are overwritten.
- Unsafe post-Link states use explicit eject/recovery rather than falsely promising Resume. Link state and multiplayer protocol are unchanged.
- Carousel `Y` stays reserved for **U2 favourites**, `X` opens Cart Studio, `SELECT` opens the core-picker animation. Do not rename/alter the game setting `X/Y Buttons` (Turbo).

## Gate

CI: `debug-apk` and `a6-a1-mgba-continuity`, Rust UI/settings/migration and Android tests, permanent APK signing fingerprint and ARM64/x86_64 package checks. Manual acceptance: Mac emulator then KONKR device, including existing SRAM/RTC/RetroArch states, Multiplayer reconnect/recovery and all live per-ROM/core options. **Do not publish a GitHub Release or merge this Draft PR before acceptance.**
