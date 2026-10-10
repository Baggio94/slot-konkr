# Dev24: compact platform names and original receiver mouth

Parent: dev23 `20fbb7fb5662853b93a6f59ae9de66429a58697c`.
Version: `0.0.7-dev24` / code 33.
Scope: visual rendering only; no changes to emulator cores, gameplay hotkeys, state files, ROMs, launcher icon (V1), BIOS, themes or splash.

1. The existing GB/GBC/GBA platform switch animation (200ms fade-in, 1200ms hold, 800ms fade-out) now reads `GBA`, `GB`, `GBC` from a dedicated `platform_abbrev` helper. All three use the same original Slot 16px label font and are rasterised at 3× texture resolution to improve filtering onto a KONKR 960×640 display, then drawn at their original display size. The original `word_face` remains unchanged for all other UI text; the platform overlay and letter-jump labels remain separate.
2. Remove exactly the two Android-only 2px bevel/shadow bands drawn inside the mouth at `SLIT_Y + SLIT_H + 1/3px` (`crates/slot-ui/src/slot_chrome.rs`). They are absent from upstream Slot v1.5.0 and were visible as a thin spurious line at the receiving slot. Retain the upstream notch and all background geometry and external shoulder treatments.
3. Update and run Rust regression checks for the platform abbreviations, 3× dimensions and pre-existing fade timing. Test `slot-ui` rendering and the RZIP module through CI.

Real KONKR QA: each of GBA, GB, GBC should reappear after every system switch at consistent text size, with sharp 960×640 appearance. Confirm the extra pale/dark horizontal groove strip is gone under the cartridge receiver. An upstream canonical edge band around the receiving mouth is intentionally preserved.
