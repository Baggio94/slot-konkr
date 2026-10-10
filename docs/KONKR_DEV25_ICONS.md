# Slot. for KONKR dev25 — original V1 HD launcher and ES-DE artwork

Based strictly on dev24 commit `71c5f4244be3a8096a26b5df98aa35697caa1587`, preserving all functionality,
selected ROM folders, core state, SRAM/RTC, hotkeys, shell rendering and original V1
appearance. Version `0.0.7-dev25`, code 34, same `fyi.slot.konkr` package.

## Icon resources (source: supplied V1 HD pack)

* Keep precisely the original approved V1 *design*, rendered from the original
  V1 SVG master (not enlarged from 192px/432px legacy PNG).
* Android 12: `drawable-nodpi/ic_launcher_foreground.png` is an **actual
  1080x1080 RGBA/sRGB PNG**. `drawable-nodpi/ic_launcher_monochrome.png`
  is the matching 1080x1080 RGBA mask. Density qualifiers must **not**
  override either file: 10 old `drawable-{mdpi,...,xxxhdpi}` foreground/
  monochrome PNGs were deleted on purpose.
* `mipmap-anydpi-v26/`: foreground and solid launcher background, valid
  for Android 12 on the KONKR. `mipmap-anydpi-v33/`: adds monochrome
  for compatible Android 13+ themed launchers. The icon manifest references
  `@mipmap/ic_launcher` and `@mipmap/ic_launcher_round` without changes.
* Legacy mipmaps mdpi/hdpi/xhdpi/xxhdpi/xxxhdpi are recreated from the
  V1 master at the correct resolutions. The optional VectorDrawables and
  ic_stat_slot are provided by the pack but **not used** as adaptive sources.
* Store separate marketing artwork at `branding/slot-for-kpa-v1-hd/`:
  `play_store_512.png` and `esde_1024.png` / `esde_2048.png`. They do
  not become launcher resources or change the app's behavior.

## CI/QA

1. Compile original Slot UI + KONKR rendering test suite and Android Rust tests.
2. Compile mGBA/gpSP ARM64 cores and test RetroArch RZIP compatibility.
3. Android aapt2 resource link and signed APK assembly; signature matches dev24.
4. Inspect final APK resource paths, validate 1080x1080 adaptive images and
   absence of density-qualified foreground/monochrome duplicates.
5. Physical KONKR launcher icon and ES-DE Android Apps must be tested after
   `adb install -r`; ES-DE may cache a small icon bitmap independently of
   PackageManager source resolution. If still pixelated, add
   `branding/slot-for-kpa-v1-hd/esde_1024.png` manually as ES-DE media.
   Do not claim actual launcher rendering tested by GitHub CI.

No source code outside Android icons, the build version, and CI branch triggers
was modified.
