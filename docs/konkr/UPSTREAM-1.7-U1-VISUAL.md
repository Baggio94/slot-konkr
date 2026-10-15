# U1 visual port: upstream Slot v1.7.0 parity

This branch is based on the frozen, publicly released `konkr-v0.1.0-rc.1` (Android code 90). It **must not** touch the public tag, RC4, saves/RTC migration, trusted peers or Multiplayer engine.

## Source and licensing

- Exact upstream source: `BrandonKowalski/slot` tag `v1.7.0`, tree `34ac38031fd903eb2a0a0157f1dea4d3a5546195`.
- All 29 `assets/*.png` source blobs are copied **bit for bit**, preserving upstream Git SHA-1, including cartridge templates, interiors, slot front/back (dark/light), link icons and wallpapers.
- `crates/slot-ui/src/{art,bitmaps,cart}.rs` derive directly from tag v1.7.0. All source licenses and original author attribution remain in effect.

## What is integrated now

1. Real cartridge asset decoding, built-in PNG templates, shell neutral-overlay blending and transparent plastic for GBA/GB/GBC.
2. The *Android rendering bridge* retains bounded async texture caching. Since original desktop Slot draws separate board/interior/transparent front textures and our Android carousel currently caches one face per cartridge, a CPU precomposition combines the upstream layers for a matching resting image. **Animated depth parallax for translucent shells is not yet pixel-identical** and should be finished in the next U1 pass.
3. Authentic `720x58` dark-theme front/back slot graphics, drawn back → cartridge → front in both rest and insertion/ejection layouts. Existing `Config/theme.txt` custom themes deliberately stay on the legacy palette rendering until a migration/opt-in UI is designed.
4. Official default shelf wallpaper when there is no user PNG. User wallpaper selection has priority; multiple user wallpapers rotate using the exact persistent v1.7.0 no-repeat deck. The official blurred variant is bundled but is not yet exposed as an interactive choice.
5. Source tests check the PNG RGBA sizes, solid-colour 1:1 faces, transparent composition and bundled wallpaper decode.

## Remaining U1 visual parity

- Exact v1.7 light-theme selection and text ink changes, theme persistence and first-run defaults.
- Wallpaper picker and tint strength `Off / Low / Medium / High`, with shuffled no-repeat wallpaper deck.
- Pixel-accurate board/interior/back/front animation including clear-cart transforms and link sprites, without breaking multiplayer.
- Replace platform selector title transition with official title-fade under Show Titles (U2 adjacent).
- Render snapshot comparisons at 720x480 and physical 960x640, including RGBA-alpha golden-output tests and D-pad timings.
- Real KONKR screenshot/audio/controller validation; CI green is **not** hardware validation.

Never replace an existing installed APK or clear app data to perform these tests. No new public release is being tagged.
