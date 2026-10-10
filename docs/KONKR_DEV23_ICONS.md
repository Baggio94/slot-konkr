# Slot. for KPA dev23 — approved original V1 artwork restored

Version `0.0.7-dev23` (code 32), based on dev22. Application ID `fyi.slot.konkr` and permanent Android signing certificate unchanged.

## Exact original V1 resources, not approximated vectorizations

The 23 `android/app/src/main/res` entries are restored byte-for-byte from the originally approved V1 icon pack (same Git blobs used in dev20). Their SHA-1 Git blob hashes were cross-checked against the V1 ZIP provided again on 2026-10-10. The V1 README explicitly says the original PNG assets are the fidelity reference and that the SVG/VectorDrawable alternatives are approximations. Therefore retain the original PNG foreground and monochrome images rather than the V4 vectors.

- Adaptive icon (`mipmap-anydpi-v26/ic_launcher.xml`, `ic_launcher_round.xml`): restored V1 XML, `@drawable/ic_launcher_foreground` and `@drawable/ic_launcher_monochrome`, separate `@color/slot_launcher_background` (#1E2126).
- Density-specific originals: foreground/monochrome in mdpi (108 px) through xxxhdpi (432 px); classic and round mipmap PNGs mdpi (48 px) through xxxhdpi (192 px).
- `android:icon` and `android:roundIcon` already reference the correct `@mipmap` resources. `compileSdk=35`, `minSdk=31` (KONKR Android 12).
- V1 Play Store art (unchanged byte-for-byte): `branding/slot-for-kpa-v1/play_store_512.png`. Marketing art is **not** used by the APK launcher. Retain historical V3 branding directory as an archive, not runtime.
- Remove unused V3 `values/ic_launcher_colors.xml` resource; V1 XML uses only `values/slot_icon_colors.xml`. Never ship duplicate resource overrides.

## Scope and checks

Only change launcher assets, the now-unused launcher color, this documentation, and the build's `versionCode`/`versionName` plus CI branch trigger. Do not edit Rust emulation, input, settings, ROM handling, saves or CRT animation.

1. CI Rust tests, mGBA/gpSP arm64 builds, RetroArch RZIP unit tests, Gradle Android resource link, APK assemble, and verification of signing certificate.
2. Confirm APK actually contains restored `drawable-<density>-v4/ic_launcher_foreground.png`, `ic_launcher_monochrome.png`, classic mipmap legacy files and adaptive icon XML.
3. Test on KONKR Quickstep/AYANEO launcher and in Android app info. ES-DE may use cached imported artwork: re-import media with overwrite or refresh its gamelist media. This is a real-device QA step, not asserted by CI.
4. Preserve `adb install -r` / never uninstall; stored ROM paths and saves remain untouched.

Android 12 can render the adaptive icon; recolored themed icon appearance needs launcher support (commonly Android 13+). The monochrome mask is packaged regardless.
