# KONKR 0.1.0 RC4 Beta 1 — Emulation diagnostics and seamless menu handoff

**Base:** RC3.1, on [PR #5](https://github.com/Baggio94/slot-konkr/pull/5). Current feature PR #6 targets PR #5 and remains **DRAFT** pending tests on the KONKR Pocket Advance. Frozen RC1 and develop/konkr are unchanged.

## User-observed problem

Input feels delayed, or some animations stutter, especially in GBA platformers Wario Land 4, Kirby games and Super Mario Advance. Separately the carousel is visible briefly between selecting **Menu → Apps/Cart Studio** and Android showing the child screen.

### What inspection demonstrates (and what it does not)

- Android `GLSurfaceView` uses continuous rendering. In each draw the Rust engine advances emulation according to elapsed wall time and the actual core FPS (typically near 59.7 for GBA). The **STANDARD** scheduler can run up to four simulation frames per presentation frame while catching up on missed vsyncs, then uploads the latest result. This can contribute to visible frame bursts if the render thread falls behind, but has not been proven to cause the user's input-to-screen lag.
- Input events reach a Rust mutex queue from Android and are applied before libretro advances during a GL draw. Thus the key hardware→Android→GL dispatch timing plus the panel/Android compositor latency also matter, independently of average emulator FPS.
- Rewind defaults to ON and takes a snapshot every six emulated frames into a background worker. It is not necessarily slow, but is a plausible CPU/memory-pressure comparison to measure.
- The renderer's real frame rate and the emulation step rate were not exposed to the user, so it wasn't possible to separate smooth 59.7 core FPS from a 30Hz screen presentation or brief catch-up spikes.
- Menu-to-activity flash was **confirmed directly in code**: app selection emitted a polled Android UI action and immediately changed `ShelfOverlay::Menu` to `None`, resulting in one or more visible carousel frames while waiting for the Android `pulse` and child Activity startup. OnPause also sent an input reset that cleared the menu.

## RC4 Beta 1 implementation

### Settings → Emulation (real original Slot renderer)
- **Show FPS** (Off default): persistent app-private setting; the in-game noninteractive top-left overlay appears after one second of measurement, and is hidden when paused/ejected or disabled. It reports real **EMU** simulation FPS versus target core FPS, **DISPLAY** GL draws per second, and **CATCHUP** percent of draws that executed more than one emulated frame. The display FPS is not guaranteed equal to physical screen refresh rate; these numbers do not measure end-to-end input lag.
- **Frame pacing**: **STANDARD** (RC3.1 scheduler, unchanged default) vs **RESPONSIVE** (opt-in). Responsive caps accumulated simulated time to **1.5 core frames** and runs at most one emulation frame per rendered screen frame unless fast-forward is active. This avoids a burst of old frames after a stall, but **can slow down gameplay** and audio on overloaded hardware. Do not promise universally lower input-to-photon latency. Settings persist using existing `slot-konkr-settings.json` with serde defaults; unit tests check the precise debt caps and legacy JSON migration.
- Original Slot quick-menu labels, A/B, D-pad changing of values and typography preserved.

### Seamless Apps and Cart Studio
- Keep native `ShelfOverlay::Menu` on-screen when selecting Apps or Cart Studio; mark a pending external child Activity.
- OnPause input reset keeps the menu only while a child screen is pending, while still clearing key states, game audio and any unrelated picker/shortcut state.
- On return via Android Activity onResume, enqueue `ExternalReturn` and clear the menu to the carousel; launch failure also triggers cleanup. The child Activity's own RC3.1 160ms enter/exit animation remains intact.
- Reuse the same logic for Apps and all-Cart-Studio menu action; Cart Studio launched directly via X is unchanged. System file chooser transitions stay OS-owned.

## KONKR manual acceptance

1. Install the **same signed** 0.1.0-rc4-beta1 APK over the current 0.1.0-rc3.1 version (versionCode 41). Never uninstall or wipe the account/RA cache, save/state data.
2. With mGBA selected, launch **Wario Land 4** and **Kirby & The Amazing Mirror**. Enable FPS counter; play for a minute in STANDARD + Rewind ON. Read/record `EMU x/y | DISPLAY z | CATCHUP n%` at idle and during intense parts.
3. Keep the same game and compare Rewind OFF (Menu → Settings → Gameplay). Then Frame pacing RESPONSIVE. Repeat on gpSP with the same ROM and matching save root; please do not reset saves and do not disable RA account without need. Compare responsiveness, audio continuity, pacing and whether game speed drops.
4. If EMU is near native ~59.7 but input still feels delayed, compare with RetroArch on the same hardware and ROM under similar audio/shader/rewind configuration, and optionally record a 120/240fps phone video showing a physical A press and the first corresponding screen response. Only then consider SurfaceFlinger/EGL pacing changes.
5. Without a game loaded, open Menu → Apps and Menu → Cart Studio several times. Confirm no brief carousel flash, both native transitions feel natural, and B/Back returns to the carousel without getting stuck in the prior menu. Also verify Cart Studio directly from X.
6. Restart Slot; confirm FPS and pacing settings persist, and no HUD appears in-game when Show FPS is OFF. Confirm A/B navigation and original control legends.
7. CI native emulator/RA/JVM tests, original Cart Studio WASM and Android arm64 APK signature must be green. Manual device sign-off required before merging DRAFT PR #6.

**Potential follow-up after measurements:** compare low-latency Android AudioTrack sizing, verify actual display refresh and GLES buffer swap cadence, profile rewind memory pressure and per-core performance, and instrument physical event-to-photon latency. Do not silently change defaults for every user based on one device.
