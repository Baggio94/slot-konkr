# slot. for KONKR Pocket Advance

An Android adaptation of [slot. by Brandon Kowalski](https://github.com/BrandonKowalski/slot), drawing on the [pvaibhav fork](https://github.com/pvaibhav/slot), for the **KONKR Pocket Advance** running Android 12 (960 × 640, 3:2). This fork is maintained separately from the Linux/H700 frontend.

**Current public release: 0.2.0.** The signed universal Android APK is built from the tested code102 feature set, with a monotonically higher Android versionCode (103). See [the 0.2.0 release notes](docs/konkr/RELEASE-NOTES-0.2.0.md).

## Supported consoles and platforms

- **Game Boy Advance, Game Boy Color and Game Boy**, using uncompressed **.gba, .gbc, .gb** cartridge files.
- KONKR Pocket Advance **ARM64**: primary physical test device.
- Mac-hosted Android emulator **x86_64**: desktop QA target using the same universal APK.
- Does not contain copyrighted games, commercial ROM sets, Nintendo BIOS or private save data.

## Features included in the Android port

- Faithful Slot cartridge carousel, system shelves, core picker, cartridge insertion/ejection visuals and sounds, screen options, customized labels and **Cart Studio**.
- Improved cartridge loading: up to 160 GPU-cached faces, bounded background prefetch, a 160 MiB compressed on-device face cache reused across app starts, and per-frame GPU upload throttling during fast scrolling.
- **10 GBA shader choices plus Off**, including LCD3x, Dot, LCD Grid V2, GBA-colour presets, zfast LCD, LCD1x, AGB-001, AGS-001 and Sharp Bilinear. Multi-pass RetroArch presets are adapted to the Android OpenGL ES renderer, not guaranteed pixel-identical.
- Built-in **mGBA and gpSP**, game/core options, fast forward, rewind, Run-Ahead where safe, controller navigation and Slot-inspired menus.
- **Save RAM and RTC**, save states, Undo and Polaroids, RetroArch-compatible SRAM and state import/export including RZIP v1, conservative conflict resolution.
- **Share mGBA/gpSP Saves**: opt-in transfer of GBA SRAM only, disabled by default. Individual save states and RTC files are not merged between cores.
- **RetroAchievements**: account status, online achievements, offline persistence/sync and in-game popups.
- **Multiplayer**: Link Cable and game/core-supported Wireless/RFU, Host/Join, local discovery and IP fallback, compatible peer checks and consent-controlled ROM sharing. Tested mGBA Cable FireRed↔LeafGreen between Mac AVD and physical KONKR; not every game/core/mode pair is supported.
- Android Apps launcher and optional launcher HOME role, status icons, Slot-styled dialogs and input shortcuts.
- Cached library instantly shown at boot; quiet background refresh, changed-ROM detection, reuse of headers and safety while games run. Scans the **whole selected ROM tree** recursively and indexes supported GB/GBC/GBA cartridge files wherever they appear.

## Install and upgrades

Get the **signed universal Android APK** from [this fork's Releases](https://github.com/Baggio94/slot-konkr/releases). The asset name includes **arm64-x86_64.apk**. Install the normal package **fyi.slot.konkr**, not the isolated Multiplayer Lab package.

Android Storage Access Framework lets you select your ROM directory and a BIOS folder where applicable. On a Mac Android emulator, the keyboard mapping is **Z → A**, **X → B**, **Enter → Start**, **Space → Select**, **Tab → Home/Mode**, arrows → D-pad. Z/X work in Slot-owned dialog menus but remain normal letters while editing account credentials or other text fields.

A normal same-certificate update with **adb install -r** preserves application data. Never uninstall the existing app, clear app data or overwrite SRAM to work around a signing mismatch. Check signing compatibility and the package identity instead.

Multiplayer ROM sharing is for **trusted local networks**: public-key peer identity authenticates devices but does not encrypt transferred content. The application does not guarantee every multiplayer mode or achievement set works for every ROM.

## Source, licensing, and attribution

Original Slot by **Brandon Kowalski**; its Rust UI and Link inspiration, including the work in **pvaibhav/slot**, are preserved and credited. See [LICENSE](LICENSE), [licenses/](licenses/) and pinned [mGBA/gpSP build scripts](scripts/) and [mGBA patches](cores/mgba/). All upstream and third-party licenses continue to apply. Android APK distribution requires corresponding source and notices for included cores.

Developer information: [0.2.0 release notes](docs/konkr/RELEASE-NOTES-0.2.0.md), [previous 0.1.0 notes](docs/konkr/RELEASE-NOTES-0.1.0.md), [qualification checklist](docs/konkr/RELEASE-READINESS-0.1.0.md) and [development PR #29](https://github.com/Baggio94/slot-konkr/pull/29).

**Android release tags are prefixed konkr-v**, such as konkr-v0.1.0-rc.1 and konkr-v0.2.0. The separate upstream Linux/H700 release process still uses v* tags.
