#!/usr/bin/env bash
set -euo pipefail
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
ADB="$SDK/platform-tools/adb"
EXPECTED="Slot_KONKR_API32"
SERIAL="${SLOT_AVD_SERIAL:-}"

avd_is_slot() {
  "$ADB" -s "$1" emu avd name 2>/dev/null | tr -d '\r' | grep -Fxq "$EXPECTED"
}

# The developer already has SleepSync AVDs. NEVER assume emulator-5554
# is the new KONKR device: several AVDs can run at once.
if [[ -z "$SERIAL" ]]; then
  while read -r candidate; do
    if avd_is_slot "$candidate"; then SERIAL="$candidate"; break; fi
  done < <("$ADB" devices | awk '$1 ~ /^emulator-[0-9]+$/ && $2 == "device" {print $1}')
fi

if [[ -z "$SERIAL" ]] || ! avd_is_slot "$SERIAL"; then
  echo "Expected AVD '$EXPECTED' not found. Launch it first; other AVDs were not changed." >&2
  exit 2
fi

"$ADB" -s "$SERIAL" wait-for-device
"$ADB" -s "$SERIAL" shell wm size 960x640
"$ADB" -s "$SERIAL" shell wm density 240
"$ADB" -s "$SERIAL" shell settings put system accelerometer_rotation 0
"$ADB" -s "$SERIAL" shell settings put system user_rotation 1
if [[ $# -ge 1 ]]; then
  "$ADB" -s "$SERIAL" install -r "$1"
  "$ADB" -s "$SERIAL" shell monkey -p fyi.slot.konkr 1
fi
printf 'Slot KPA AVD ready on %s. Other virtual devices were not modified.\n' "$SERIAL"
