# Slot. for KONKR — dev26 Cart Studio foundation (2026-10-10)

## Baseline and provenance

- Parent: approved PR #1 merge `ae58da042964130c90dad93ba475f0364f6541f2` into `feat/konkr-dev25-v1-hd-icons`; approved V1 HD icons, Android 12 splash and Recents fix preserved.
- Development branch: `feat/konkr-dev26-cart-studio-foundation`.
- New version: `0.0.7-dev26-foundation`, code 35, unchanged package `fyi.slot.konkr` and persistent signing certificate.
- Official Studio source: [BrandonKowalski/slot-cart-studio](https://github.com/BrandonKowalski/slot-cart-studio), **pinned commit** `af7c27e84fe79bb12537d825ff781474f96e9256`, imported without changes as 45 files in `third_party/slot-cart-studio/`. It includes the unmodified Rust engine, JavaScript editor, CSS, tests, ScreenScraper harvesting scripts, and its GPL-3.0-or-later license. The upstream Studio records original Slot pin `3b907510201c70e579aff7d9205056d18dd0618d` in `slot.ref`.
- This import is **source only**. The upstream browser needs a generated `web/pkg/slot_cart_studio.js` and `.wasm` built with wasm-pack; they are **not yet packaged into the Android APK**. Do not claim the graphical Studio is active yet.

## First functional integration: ROM identity, read-only

- `CartStudioCatalog.kt`: consumes the existing app-private `rom-library-cache-v1.json` instead of asking for a slot SD card; reads existing SAF `content://` ROM URIs; computes **CRC32 over the entire ROM** and captures **the first 0x150 header bytes** in the same buffered stream, matching upstream `web/studio.js` / `src/rom.rs`. Never copies or edits a ROM.
- App-private, atomic cache `cart-studio-crc-cache-v1.json` records CRC/head keyed by content URI, size, and modified timestamp. Only reuse values for positive stable metadata; invalid / absent metadata forces streaming anew.
- Main carousel -> START -> Scraping -> A now identifies the selected cart asynchronously, showing platform and 8-character uppercase CRC32. This is an **identity proof-of-integration**, **not yet label scraping or the full Studio editor**.
- `crates/slot-android/src/lib.rs` includes the exact original `third_party/slot-cart-studio/src/dat.rs` module as a path dependency. Its original No-Intro DAT parsing, name search, and official tests run with `cargo test -p slot-android --lib`; future steps connect it to the matching and artwork pipeline.
- Kotlin unit tests verify reference CRC32 `CBF43926`, first-0x150 header, short reads, and stream chunk invariance.

## Next dev26 implementation steps (not delivered yet)

1. Build pinned official Rust/WASM renderer with its own `slot.ref`, using wasm-pack in CI; package the official HTML, JS, CSS and WASM in Android assets. Reconcile versions of upstream Slot's Rust render model before attempting to mix Rust modules from different commits.
2. Use a **local-origin WebView**, with an explicitly restricted native bridge, to provide the already-scanned Android library to the exact Studio UI. Replace the original SD picker adapter `web/card.js`, not Studio's rendering logic.
3. Connect per-ROM labels to existing app-private `files/Labels/<FNV1a-of-content-URI>.png` and already-working `nativeReloadCartLabel(uri)`; never mutate ROM/save/state files.
4. Store the official shell choices (outline, colour, finish Solid/Clear/Glitter) as app-private per-ROM overrides. The Android `library.rs` currently supplies provisional GB/GBC shells; adapt this so automatic original shell selection and game-specific overrides (e.g. Crystal glitter) actually work with SAF ROM header data before declaring fidelity.
5. Add No-Intro metadata fetch, `art.slot-cfw.fyi` art index, real labels and logo-only generation, caching, error handling, batching and manual unmatched-ROM selection. Artwork licenses may differ: ScreenScraper images are CC BY-NC-SA 4.0, so preserve attribution and commercial-use restrictions.
6. Physical KONKR QA: touch/gamepad navigation, per-game reopen, shader/CRT, memory on large collections, offline cache, launcher splash and Recents regression.

## Preservation and test policy

Do not merge dev26 into `main`, uninstall the app, change the approved V1 icon, or delete any settings/saves. CI is necessary but insufficient for real launcher or WebView behavior. Native bridge must never expose unrestricted file/ROM access to arbitrary remote websites.
