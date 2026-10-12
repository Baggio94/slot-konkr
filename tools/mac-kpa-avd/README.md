# Slot. KONKR Pocket Advance — Mac Android Test Device

This is an isolated **Android 12 / API 32**, **960×640 (3:2)** virtual handheld for testing the same Slot APK used on the physical KONKR.

Requires an **Apple Silicon (arm64) Mac**; the current Slot APK bundles Android ARM64 mGBA/gpSP libraries, so a standard x86_64 AVD on an Intel Mac cannot load them. Does **not** replace or wipe existing SleepSync_API35 or other Android virtual devices.

## Setup on Mac

Install Android Studio and the Android Emulator/SDK Command-line Tools (latest), then in a Terminal from the repository root:

```bash
bash tools/mac-kpa-avd/setup.sh
"$HOME/Library/Android/sdk/emulator/emulator" -avd Slot_KONKR_API32 -gpu auto -no-boot-anim
```

After emulator boot (use a second Terminal):

```bash
SLOT_AVD_SERIAL=emulator-5554 bash tools/mac-kpa-avd/configure.sh /path/to/signed-slot.apk
```

The setup creates a new AVD with the correct aspect ratio, API level, keyboard input and OpenGL renderer. It modifies **only** the new virtual device configuration.

## Keyboard controls

Mappings are implemented inside Slot and enabled **only on detected Android emulators**, not on physical KONKR devices.

| Mac keyboard | Simulated KONKR input |
|---|---|
| ↑ ↓ ← → | D-pad |
| Z | A |
| X | B |
| A | X |
| S | Y |
| Q / E | L1 / R1 |
| 1 / 2 | L2 / R2 |
| Return | Start |
| Space | Select |
| Tab | Mode / round menu |

Press and hold keys for continuous in-game or carousel input; mapped press and release events are passed to the same native game controller path. The mapping adds no Android permissions.

## ROMs, BIOS and saves

Slot uses Android's normal system file pickers. After installing, copy **user-owned, legally obtained** test ROMs/BIOS into the AVD's shared Downloads folder with `adb push`, then select the containing folders from Slot's settings. For meaningful regression testing, use copies of test saves, **never the physical KONKR's only copies**.

## Wi-Fi Multiplayer — physical KONKR ↔ Mac emulator

This is a **planned test scenario, not yet proven**. Android Emulator generally uses a NAT router with internal addresses (often 10.0.2.x), does not support IGMP and may not relay mDNS multicast. Its virtual Wi-Fi is **not the physical KONKR's LAN Wi-Fi**. Google documents this in the [emulator networking guide](https://developer.android.com/studio/run/emulator-networking-address).

For a first real paired test: **Host on the physical KONKR; Join on the emulator with an explicitly entered KONKR LAN IPv4 address.** This needs an upcoming `Join by IP` fallback in the native transport; ordinary discovery alone is unreliable in the emulator. Outbound TCP from emulator through Mac's LAN should then reach the KONKR provided routing/firewall allow it. Do not claim that the existing auto-discovery Host/Join flow proves a working link.

When testing, use the same signed Android Slot APK on both endpoints and use gpSP for genuine GBA Cable/RFU (mGBA Wi-Fi fork remains deferred).
