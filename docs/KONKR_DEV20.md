# Slot. for KPA — dev20: platform names, approved launcher icon pack, KONKR boot logo, BIOS toast

Base: dev19; package remains `fyi.slot.konkr`; no ROM, save, emulator or input changes.

- Use Slot v1.5.0 `Platform::name()` to draw **Game Boy Advance**, **Game Boy**, and **Game Boy Color** in the exact existing Slot bottom cartridge mouth. Match original 200 ms fade-in, 1200 ms hold, 800 ms fade-out, peak alpha 0.4; preserve existing letter-jump label. Skip empty platform shelves, just as dev19.
- Copy the official user-approved asset pack **PNG files byte-for-byte**, without substituting the approximate vector artwork. Five densities of foreground and monochrome, legacy regular/round mipmaps, adaptive icon XMLs and the exact solid background. Declare `android:icon` and `android:roundIcon`. compileSdk 35, minSdk 31 (Android 12).
- Keep original `assets/slot-bootlogo.bmp` as pristine source and use an app-only **960x640** PNG, rasterized from original Slot font/art with high-quality resampling and sharpened antialiasing; no firmware modifications. Splash fades when GL is ready.
- Import configured BIOS silently on normal startup, and show the existing `BIOS ready` message **only upon first explicit BIOS folder selection**, not every app launch. Failures remain visible.
- Bump only `versionCode` 29 / `versionName` `0.0.7-dev20`.

## Review checklist

1. Confirm Gradle `:app:processDebugResources` and `:app:assembleDebug` pass; `aapt2` adaptive and monochrome resources are correctly linked and manifest references remain valid.
2. Confirm Rust Slot Android library tests and the new platform alpha-duration regression test pass.
3. Confirm APK signer matches the existing persistent certificate, and run `adb install -r` (never uninstall).
4. On KONKR, inspect launcher regular and round icons, app icon after upgrade, dynamic masks, shelf platform names on L1/R1 and boot splash sharpness.
5. Verify silent BIOS restore and first-time BIOS folder selection message. No other behaviour changes intended.

Android 12 draws the adaptive launcher icon, but Android's themed Material You launcher icons usually require Android 13 and a launcher with support. The monochrome mask is nevertheless packaged for those launchers.
