# A12 Multiplayer mGBA Cable diagnostic A/B
Based on A11 09962e4cda62aa862c7e5737835ece7e3827fdfb. Default mode: A11 bounded catch-up (up to 2 real
linked frames per GL callback). Diagnostic-only private flag file selects
legacy pacing. No change to KMLB/4, the 3-frame input delay, gpSP RFU/Cable,
Pokémon FLASH/save protection, or any solo emulation settings.

## A/B configuration on KONKR and Mac AVD
Connect both with ADB. To enable legacy (A11 recovery OFF) **on both**:
```bash
ADB="$HOME/Library/Android/sdk/platform-tools/adb"
for serial in BW0308N250009576 emulator-5556; do
 "$ADB" -s "$serial" shell am force-stop fyi.slot.konkr
 "$ADB" -s "$serial" shell run-as fyi.slot.konkr touch files/a12-mgba-legacy-pacing.flag
done
```
To restore A11 recovery (A12 default):
```bash
for serial in BW0308N250009576 emulator-5556; do
 "$ADB" -s "$serial" shell am force-stop fyi.slot.konkr
 "$ADB" -s "$serial" shell run-as fyi.slot.konkr rm -f files/a12-mgba-legacy-pacing.flag
done
```
After each change, manually reopen Slot on both and start a NEW Link
session. Check Android logcat for `A12 recovery mode: a11` or `legacy`.
The marker is app-private and does not alter any game/RetroArch save.
Use `bash scripts/slot-kpa-a12-link-diagnostics.sh` to collect logs after
5–10 minutes of F-Zero mGBA Cable, with A11 recovery ON vs OFF.

## Metrics
- `Link pacing`: emulated FPS, peer input waits, mGBA core timings.
- `Link display pacing`: emulated frames produced (0,1,2) per native draw.
- `GL callback pacing`: callback gaps >25/40ms and native draw >16/33ms.
- `A12 PCM pacing`: how many audio shorts were supplied vs expected,
  empty reads and AudioTrack underruns, plus one PCM queue sample.
Note: GLSurfaceView timing is *before* EGL swap, NOT exact display
presentation timestamps. If input delay 3→5 frames is later tested,
it needs explicit joint handshake validation; A12 leaves it unchanged.

Do NOT install without backing up private Saves, States and preferences.
App-private backup does not include external RetroArch/Syncthing SAF data.
