# Slot. for KONKR — dev26 original Cart Studio Android preview

**Base:** hardware-approved PR #1 V1 HD icons, native splash and Recents fix.
**Development:** draft PR #3, no merge into main.
**Version:** \`0.0.7-dev26-studio-preview\` (code 36) / existing signing key.

## What is implemented

The original, pinned [Slot Cart Studio](https://github.com/BrandonKowalski/slot-cart-studio)
HTML/CSS/JavaScript and its original Rust/WebAssembly renderer are built at the
original Slot source revision, with Rust 1.96.0 and wasm-pack 0.15.0, and then
packaged as app-private assets under \`studio/\`.

On the original Slot. shelf select a cart → START → Scraping → A → a separate
landscape, touch-capable Cart Studio activity opens. There is no Android SD picker.
The app supplies **only the selected indexed ROM**, its 0x150-byte header and
streaming CRC32, from the existing Android SAF library. The original matching
logic, Real Label and Logo Only layouts, preview, undo/redo, shell/finish choices
and editor remain upstream; the sole generated JS modifications import the
KONKR adapter, skip duplicate CRC reading by using the already computed checksum
and open the selected cart at startup.

WebView local origin: \`https://appassets.androidplatform.net/studio/index.html\`.
The APK intercepts only its packaged local resources; the JavaScript bridge is
restricted to one selected game and does not accept arbitrary ROM URIs/paths or
write to ROMs or saves. External navigation is blocked, CSP prohibits frames and
remote scripts. Remote *artwork and DAT* reads are from original Studio services,
so the first load needs network unless those services have been cached by WebView.
This is **not yet an offline scraping engine or a full-library batch importer**.

Saved label PNG is strictly validated (PNG header, dimensions, max 2 MiB),
atomically written to the same app-private \`Labels/<FNV1a(uri)>.png\` as
the existing manual label feature. Per-ROM shape, colour and Solid/Clear/Glitter
are saved atomically under \`Studio/<FNV1a(uri)>.shell\` and reapplied to
the original Rust shelves using the optional \`shell_override\` field. The
existing ROM index cache remains unchanged; native data and textures refresh
on return from the editor. Existing saves, BIOS, input bindings, animations,
settings and approved V1 branding remain untouched.

## Verify on real KONKR

1. Upgrade with \`adb install -r\`, **never uninstall**.
2. Select a GBA/GB/GBC game and use START → Scraping → A.
3. Verify the genuine Cart Studio shows the selected game without asking for SD.
4. With Wi-Fi on, verify No-Intro match, Real Label and Logo Only previews.
5. Change shell colour/outline/finish, save (top right), close Studio, verify
   cartridge updated and preference survives app restart.
6. Generate a label, save it, return to shelf, verify label still appears after
   restart and undo/revert are functional inside the editor.
7. Exit via top-left SLOT. button or hardware B and verify controls,
   gameplay, saves, audio, original boot splash, Recents and ES-DE icon.
8. Also test Wi-Fi off and a ROM hack/unmatched CRC: expected graceful missing
   art/fallback, no crashes or unwanted user-data writes.

### Important limitations

- The first integration shows **one selected cart**. The original full
  collection view, bulk generation, multi-platform scraping queues and
  persistent offline DAT/artwork cache will be added after hardware tests.
- The original Studio source (45 files) is vendored without modification
  with GPL-3.0-or-later LICENSE and separate source notices for DAT/logos.
- The original renderer's version is pinned to its \`slot.ref\`; the CI
  deliberately builds that revision, *not* the newer Android fork renderer.
  Visual parity should be checked with real GB/GBC/GBA examples.
- Android WebView cannot assume gamepad browser-focus support equals a native
  gamepad UI; touch is the initial supported input method, physical B exits.
- Do **not** merge the draft PR until physical validation on KONKR.
