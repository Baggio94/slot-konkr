#!/usr/bin/env bash
set -euo pipefail
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
ADB="$SDK/platform-tools/adb"
SERIAL="${SLOT_AVD_SERIAL:-emulator-5554}"
"$ADB" -s "$SERIAL" wait-for-device
"$ADB" -s "$SERIAL" shell wm size 960x640
"$ADB" -s "$SERIAL" shell wm density 240
"$ADB" -s "$SERIAL" shell settings put system accelerometer_rotation 0
"$ADB" -s "$SERIAL" shell settings put system user_rotation 1
if [[ $# -ge 1 ]]; then
  "$ADB" -s "$SERIAL" install -r "$1"
  "$ADB" -s "$SERIAL" shell monkey -p fyi.slot.konkr 1
fi
printf 'Slot KPA AVD ready on %s. Keyboard mapping is active only in emulator builds of Slot.\n' "$SERIAL"
