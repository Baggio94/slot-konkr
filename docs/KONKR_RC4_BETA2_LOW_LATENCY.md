# KONKR RC4 Beta 2 — opt-in latency experiments and per-game tuning

**Package:** `fyi.slot.konkr` — **versionName:** `0.1.0-rc4-beta2` — **versionCode:** `42`  
**Branch / draft PR:** `feat/konkr-emulation-latency-diagnostics` / PR #6, based on RC3.1 / PR #5.

## UI
- **Menu → Settings → Emulation** (original Slot 720×480 skin): Show FPS, Frame pacing STANDARD/RESPONSIVE, Run-Ahead OFF/1/2 frames, Frame Delay OFF/1/2 ms, GPU Sync OFF/ON, Audio Latency LEGACY/32/48/64/96 ms. No new shortcut in active gameplay.
- On the **cartridge carousel**, **Y** opens the same options **for the highlighted game**, with **Reset to global** as the last row (press A). Left/right changes values. B closes the menu. **X still opens genuine Cart Studio** and SELECT still selects the cartridge's libretro core.
- A per-game profile is created only after the first setting is edited and stored under `files/GameEmulation/<uri-key>.json`. It inherits the global values until changed, and resets by deleting only that profile. The game/URI and existing ROM, SRAM, states, RA data and account remain untouched.
- Apps controller footer is **40 dp**, keeping the exact original PNG hint sizes/spacing and Gravity.CENTER.

## What the experiments actually do

| Option | Implementation | Default / caveat |
| --- | --- | --- |
| Show FPS | Existing RC4 real EMU/DISPLAY/CATCHUP counters | OFF. Does **not** measure input-to-photon delay. |
| Frame pacing | Existing RC4 limited simulation debt | STANDARD. RESPONSIVE can slow gameplay or distort audio under load. |
| Run-Ahead | After all real libretro frames/audio have been processed, serialize core state, simulate up to 2 **speculative** frames, copy the last frame's pixel buffer, discard speculative audio and restore the original state. New RetroCore methods bypass RetroAchievements tracking for speculative frames and their restoration. | OFF. GBA only in this initial beta. May cost substantial CPU and introduce incorrect video with some cores. Do not use when fast-forwarding or rewinding. Automatically stops attempting if serialize/restore fails. |
| Frame Delay | Bounded experimental 1–2 ms GL-side delay followed by a **late drain of pending Android key events** before core execution. | OFF. This is **not** RetroArch's calibrated automatic VSync frame delay; without actual device measurements it could worsen smoothness. |
| GPU Sync | GL thread calls `glFinish()` after drawing to bound the GPU work in flight. | OFF. Can stall the GL thread; not the same as RetroArch's desktop GPU hard sync. |
| Audio Latency | Configures Android `AudioTrack` stream buffer at the selected requested number of milliseconds, bounded by Android's mandatory minimum. The audio worker rebuilds when the preset changes. | LEGACY preserves the previous max(16,384 bytes, 2× HAL minimum). Lower presets can crackle. |

**Not yet implemented:** genuine RetroArch **Preemptive Frames** input-change replay, calibrating automatic frame delay to VSYNC / SurfaceFlinger or a deterministic end-to-end input→pixel tracer. Those need separate instrumentation/architecture tests. No menu option pretends they already work.

## KONKR acceptance checklist

1. Back up app data if experimenting with core state features; **install over** previous RC with `adb install -r`, never uninstall or replace the signing key.
2. Wario Land 4, Kirby & the Amazing Mirror, Super Mario Advance 4: baseline **mGBA + STANDARD + Rewind ON + Run-Ahead OFF + Frame Delay OFF + GPU Sync OFF + Audio LEGACY**. Measure EMU/DISPLAY/CATCHUP and feel.
3. Same scene: Responsive ON/OFF; Rewind OFF; Run-Ahead 1 then 2; Frame Delay 1/2; GPU Sync ON; Audio 48/32. Change **one option at a time**, note sound speed/crackles, motion and frame counters; repeat on gpSP.
4. Verify **Y** from cartridge and that every title has its own persisted settings; **Reset to global** recovers the current global settings. Restart Slot and confirm persistence.
5. Verify original **X Cart Studio**, SELECT core picker, Apps footer 40 dp, handoff Menu→Apps/Cart Studio and return B/Back.
6. Verify saving, loading, rewind, offline RA, actual awards and pending sync with Run-Ahead ON **without speculative unlocks**. Do not mark beta as stable from a green CI alone.
7. If GPU Sync/Frame Delay degrades FPS or feels worse, switch them back OFF. If audio crackles, select LEGACY immediately.

**Status:** Code changes require a green full Android ARM64 CI build, signed APK validation, and actual KONKR acceptance before merging PRs or calling the latency fixed.
