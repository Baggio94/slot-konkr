# Dev22 — official Slot. for KPA Android icon pack v3
- Base: dev21 (6d186f48). Version 0.0.7-dev22, versionCode 31.
- Launcher icon: use v3's mipmap legacy PNGs and separate foreground from `exports/adaptive_foreground_1024.png`, scaled into Android drawable density-qualified assets (108/162/216/324/432 px). The visual remains within the canonical 66% adaptive safe circle.
- Android adaptive icon XML is the exact pack v3 resource; background `#141B23` is supplied by `@color/ic_launcher_background`.
- The pack v3 monochrome PNG/SVG preview is opaque dark and is not an Android alpha mask. The Android drawable is generated from the same v3 approved foreground artwork as **a transparent white-alpha silhouette**, with dark voids punched out. This avoids solid-square tinted icons. Theme providers can tint alpha according to the wallpaper.
- Google Play icon `branding/slot-for-kpa-v3/play_store_512.png` is separate, 512×512 full-bleed 32-bit PNG, never referenced as the device launcher icon. The source master/SVG, plus the supplied README_FR.md, are preserved for provenance.
- Legacy icons are the unchanged v3 PNGs for mdpi/hdpi/xhdpi/xxhdpi/xxxhdpi.
- `AndroidManifest.xml` already points to `@mipmap/ic_launcher` and `@mipmap/ic_launcher_round`. No other application code was changed.
- Android target minSdk 31, targetSdk 32, compileSdk 35: adaptive launcher icons supported on KONKR Android 12; themed system icons require launcher/OS support, generally Android 13+. `<monochrome>` is packaged to allow compatible launchers.
- CI builds all cores and APK, validates Android resource linking and verifies the permanent signing certificate. Runtime launcher screenshots need physical KONKR for final confirmation.
