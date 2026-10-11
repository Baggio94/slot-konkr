# KONKR RC4 Beta 6 — experimental display synchronization

## Why
RC4 Beta 4's two-frame recovery increased reported catch-up and input lag. Beta 5 restored the preferred single-frame RESPONSIVE. Beta 6 **does not modify that behavior**. RetroArch on the KONKR was observed with Vulkan (OpenGL felt nearly as good), Threaded Video OFF, VSync ON, single-instance Run-Ahead 1, auto frame-delay OFF. A Vulkan rewrite is **not** included.

## New opt-in frame pacing choice
`Menu → Settings → Emulation → Frame pacing` or shelf Y per-game cycles:
- STANDARD — original catch-up scheduler, unchanged.
- RESPONSIVE — one real frame per GL callback, 1.5-frame maximum accumulated debt, unchanged from Beta 5.
- DISPLAY SYNC — **experimental**; the same responsive scheduler, but Android `Choreographer` requests exactly one `GLSurfaceView.RENDERMODE_WHEN_DIRTY` drawing callback per screen vsync. OpenGL ES is still the renderer.

Legacy JSON files continue to load: `responsive_pacing=true` remains RESPONSIVE. New `display_sync_pacing=false` defaults off. `DISPLAY SYNC` implies `responsive_pacing=true`. UI toggling uses the existing row; no extra menu items. Swapping modes needs no process restart. On pause, restore continuous render mode before pausing the GL thread; on resume, restart the Choreographer callback. If you see frozen animations in DISPLAY SYNC, choose RESPONSIVE again.

## Measurements (Show Stats)
- EMU / DISPLAY / CATCHUP and existing CORE/AHEAD/UP/GL are retained.
- Added FRAME mean (ms), P95 interval (ms), LATE count (observed GL callback intervals exceeding 1.5× nominal refresh interval out of 60 samples).
- A-UND is the Android AudioTrack cumulative underrun counter since AudioTrack was created.
- These **are not** measurements of display scan-out, actual hardware GPU completion, swap-buffer time, or input-to-photon latency. Frame callback frequency is not exactly panel presentation frequency. If FRAME P95 or LATE worsens, that mode is less smooth even if EMU FPS rises.

## Controlled KONKR comparison
Use the same Wario Land 4 segment (then Kirby and Super Mario Advance), mGBA, Rewind OFF, Run-Ahead 1, Frame Delay OFF, GPU Sync OFF, legacy audio first. Compare RESPONSIVE against DISPLAY SYNC in alternating 90-second runs. Note EMU/DISPLAY/CATCHUP, FRAME mean/P95/LATE, A-UND and especially perceived jump/input lag. Do not chase exact EMU=59.7275 at the expense of perceptual smoothness. Re-test Run-Ahead OFF to isolate speculative CPU cost, then compare mGBA vs gpSP.
No new Vulkan driver, automatic frame delay, full RetroArch dynamic-rate audio time stretching, or second-instance run-ahead in Beta 6. Do not merge until physical KONKR acceptance.
