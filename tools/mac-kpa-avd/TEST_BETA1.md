# slot. for KONKR Pocket Advance — Mac AVD 1.2.0 beta1 test harness

This branch hosts the **macOS Android emulator tools**, not a second copy
of the 1.2.0 source code. The tested APK comes from the
[`beta/1.2.0` branch of `slot-for-kpa`](https://github.com/Baggio94/slot-for-kpa/tree/beta/1.2.0).
Do not build this older `slot-konkr` Android tree and call it beta1.

## Mac prerequisites

- Apple Silicon Mac (`uname -m` must return `arm64`)
- Android Studio, Android SDK platform-tools, emulator and command-line tools
- An independent AVD called `Slot_KONKR_API32`; never alter your other AVDs
- Official signed beta1 APK or CI ZIP (no ROMs or BIOS included)

First-time setup from this branch:

```bash
bash tools/mac-kpa-avd/setup.sh
"$HOME/Library/Android/sdk/emulator/emulator" -avd Slot_KONKR_API32 -gpu auto -no-boot-anim
```

In another Terminal after Android boots:

```bash
bash tools/mac-kpa-avd/configure.sh
bash tools/mac-kpa-avd/test-beta1.sh "$HOME/Downloads/app-release.apk"
```

Or provide the downloaded GitHub Actions ZIP directly as the last argument.
If a `gh` CLI is installed, get the successful run artifact from
[slot-for-kpa Actions](https://github.com/Baggio94/slot-for-kpa/actions)
(`beta/1.2.0` branch, artifact `slot-for-konkr-1.2.0-beta1-arm64-signed`).

The install script validates the AVD name and ARM64 ABI before installing,
preserves application data, and **never uninstalls**.

## Keyboard

| Mac | Virtual KONKR |
| --- | --- |
| Arrows | D-pad |
| Z / X | A / B |
| A / S | X / Y |
| Q / E | L1 / R1 |
| 1 / 2 | L2 / R2 |
| Enter | START |
| Space | SELECT |
| Tab | slot. cap (insert/eject / in-game menu) |

Keyboard mapping is **emulator-gated in the real beta source**, not active on
physical KONKR devices. For the 1.2.0-beta1 AVD tests, install the **latest**
CI artifact produced after restoring the emulator mapping.

## Focused beta test procedure

1. Choose a folder containing a few **copies** of GBA, GB and GBC test ROMs.
2. Browse quickly; the scrolling and system switching must feel unchanged.
3. Idle on a cartridge for 1–2 seconds; check for `SlotRomWarm` messages.
4. Launch, eject and relaunch the same cartridge; compare
   `Slot launch preparation: ROM cache=...ms` log measurements.
5. When online and idle, check automatic artwork for games with **no**
   custom label; verify exact game/platform, correct layout and no flashing.
6. Play offline and check RetroAchievements cached behavior.
7. Verify RetroArch-compatible save/state continuity **using test copies**.
8. Lock/resume, reopen, change core, and inspect crash/performance logs.

Create a diagnostic file:

```bash
"$HOME/Library/Android/sdk/platform-tools/adb" devices
# Replace SERIAL with your Slot_KONKR_API32 emulator serial only:
"$HOME/Library/Android/sdk/platform-tools/adb" -s SERIAL logcat -d -v threadtime \
  -s SlotKonkr:I SlotRomWarm:I SlotAutoLabels:I > ~/Desktop/slot-kpa-beta1-avd.log
```

**Limitations:** AVD storage and GPU speed differ from KONKR hardware. It
cannot validate real battery drain, top-button ergonomics, haptics, OLED
appearance or real LAN Wi-Fi peer discovery. Final measurements still
require the KONKR Pocket Advance.
