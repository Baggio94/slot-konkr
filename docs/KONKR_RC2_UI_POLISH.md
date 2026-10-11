# KONKR slot. 0.1.0 RC2 — Cart Studio / Apps / motion polish

**Based on:** `develop/konkr` at consolidated RC1 commit `eca5a3b0`. **Target:** KONKR Pocket Advance, Android 12, 960×640. **Candidate, not stable.**

## User's RC1 screenshots and actionable fixes

### 1. Cart Studio action bar
The RC1 image shows `Cart Studio` visually overlapped by `Fill unlabelled` when the WebView starts reporting an active `Save to slot.` action. Source root cause: the title was a full-width `FrameLayout` child and both JS-derived action buttons sat in a separate right-aligned overlay, allowing them to draw on top of the title. Further, a hidden action used `GONE`, causing the adjacent button to jump when its visibility toggled.

**RC2 fix**: lay out Back + weighted Title + reserved Fill/Save actions as non-overlapping siblings in a horizontal row; use `INVISIBLE` (not `GONE`) when an action is disabled so each action retains its measured footprint. Preserve the exact official Studio JavaScript action semantics and both actions when valid; no arbitrary timeout or delayed hiding. The original Studio WebView, source, save logic, and action bridge are untouched.

### 2. Apps footer / scrollbar, previous PR #3 backlog
The source had `root` bottom padding 15dp and controls footer top/bottom 8dp/7dp; the GridView scrollbar had not been disabled.

**RC2 fix**: root bottom 4dp and footer top/bottom 3dp/2dp. **Do not touch** the PNG assets in `apps_controls`, their bitmap sizes, scaled widths, spacing, typefaces or keycap graphics. Suppress vertical and horizontal platform scroll indicators, disable platform overscroll glow but **retain GridView scrolling and existing `keepSelectedRowVisible` gamepad selection logic** (including partially visible selected rows). Screen still scrolls through arbitrarily many apps via D-pad.

### 3. Shared opening/closing motion
The original native Slot menu uses `slot_ui::ease` quintic smootherstep, with an intro of 0.16 s (`crates/slot-android/src/runtime.rs`, `draw_original_style_menu`). App-owned Android Activities previously used their default unrelated system transitions; account dialogs likewise used the OS defaults.

**RC2 fix**: a shared Android transition vocabulary, 160ms paired entrance and exit with 8dp understated y-travel and an Android pathInterpolator approximating the quintic ease. Apply to **Apps** and **Cart Studio** when launched from the Slot shelf and on B/Back/close; apply to the account dashboard and account-modal windows. Do **not change** the actual Rust menu animations, cartridge insert/eject timing or already hardware-approved sounds, other applications or OS file/permission dialogs. Android interpolation is an approximation of `slot_ui::ease`, not a binary-identical easing curve.

## Validation before merging into develop

- Inspect Cart Studio screenshot at 960×640: title unoccluded during `Fill unlabelled` -> modified -> `Save to slot.`; both controls remain in fixed locations and work; Save/Fill state toggles do not shift either label.
- Open Apps: original MOVE / A OPEN / B BACK images identical to RC1; shorter footer and no visible scrollbar; fully visible selected row using D-pad in long grids; Set as Home behavior unchanged.
- Open/close Apps, Cart Studio and RA dashboard: synchronized 160ms motion, no black flash, input delay, WebView focus regression or screen edge clipping. Verify B and X return and persistence after Studio save.
- Sanity test GBA mGBA/gpSP and offline queued RetroAchievements from RC1: no changes in RA backend, cart audio, save/state logic.
- CI: native Rust emulator/RA/unit tests, UI fidelity, Kotlin/JVM tests, actual WASM Cart Studio packaging, ARM64 GBA cores and persistent APK signing. Version name `0.1.0-rc2`, code `38`, application ID `fyi.slot.konkr`.
- **Keep RC1 branch unchanged**, PR draft until KONKR acceptance. On success, merge targeted correction PR into `develop/konkr` only. Do not merge RC into `main` without explicit acceptance.
