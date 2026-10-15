# slot. for KONKR Pocket Advance — 0.1.0 release notes

This is the first complete Android KONKR Pocket Advance release family. One signed APK includes **ARM64** libraries for the handheld and **x86_64** libraries for Mac-hosted Android emulation.

## All included features

- Game Boy Advance, Game Boy Color and Game Boy ROM support, embedded mGBA and gpSP, system shelves and game/core options.
- Original-style cartridge graphics, animations, insertion/ejection sound, controller-friendly menus, customization and Cart Studio.
- Core-specific Run-Ahead safeguards, fast-forward, rewind, SRAM, RTC, separate states, Undo, Polaroids and RetroArch-compatible save import/export.
- Opt-in mGBA↔gpSP GBA SRAM sharing and explicit conflict handling; saves never silently reconciled on differing histories.
- RetroAchievements with online/offline progression, in-game notifications and account management.
- Host/Join Multiplayer, supported Cable/RFU link modes, discovery, manual IP fallback, session isolation and explicit permission for sending a peer ROM.
- Apps launcher, HOME role, status integrations, dialog keyboard/gamepad navigation and full recursive ROM index refreshing with cache.

## Setup and limitations

Install the normal **fyi.slot.konkr** signed APK; do not install the isolated Lab sidecar over production. A compatible signed update via **adb install -r** retains data; do not uninstall or clear application data to force a signature mismatch.

ROMs, BIOS, private saves and RetroAchievements login details are **not included**. Only GB/GBC/GBA cartridge file formats are indexed, though files may be under any directories.

Multiplayer is for trusted LANs: Ed25519 authenticates peers but does not encrypt their traffic. Actual working link pairings depend on ROM/core/mode; mGBA Cable FireRed↔LeafGreen has been tested, but do not assume universal multiplayer compatibility.

## Provenance

This build derives from original Slot by Brandon Kowalski and the pvaibhav fork. Pinned mGBA and gpSP source revisions, licensing and patch/build scripts are described in the source repository. See the Android release readiness checklist for qualifying this candidate on real hardware.
