# Cart Studio — Android milestone 2: upstream WASM build

Physical KONKR dev26 Foundation validation (2026-10-10): \`START → Scraping → A\` successfully displays the selected ROM CRC32; recorded on draft PR #3.

The original Studio [BrandonKowalski/slot-cart-studio](https://github.com/BrandonKowalski/slot-cart-studio) is pinned under \`third_party/slot-cart-studio/\` without modifications. Its Rust renderer deliberately imports Slot sources *by relative path*, and the \`slot.ref\` SHA must be respected. It must not simply import sources from our Android fork, which may differ from the exact renderer revision Studio expects.

Dedicated GitHub Actions [\`konkr-studio-wasm.yml\`](../.github/workflows/konkr-studio-wasm.yml):

1. Clone exactly the official Slot commit pinned by the Studio \`slot.ref\`, place \`slot\` and a copy of our archived \`slot-cart-studio\` beside one another.
2. Execute the original \`web/*.test.mjs\` unit tests with Node.
3. Compile unchanged Rust/WASM with the official Rust 1.96 + wasm-pack 0.15.0 toolchain.
4. Publish \`web/pkg/slot_cart_studio.js\` and \`slot_cart_studio_bg.wasm\`, together with the original editor UI, as an archived CI artifact.

This validation milestone is **not yet an Android UI release**. No part of the Studio editor is enabled inside the APK at this step. Next, integrate the locally compiled files with a local-origin WebView and an access-controlled Android SAF adapter, rather than giving the JS arbitrary filesystem access. Preserve a controller/touch-friendly portrait-free 3:2 layout, original UI, browser attribution, and ROM read-only safety.

Scope of this milestone: only the dedicated WASM CI, documentation, and tests; no changes to emulation, persistence, launcher V1 graphics or saves.
