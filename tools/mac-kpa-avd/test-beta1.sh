#!/usr/bin/env bash
# Install the real slot-for-kpa beta on the isolated Mac KPA AVD.
# No source fork and no physical KONKR changes.
set -euo pipefail

if [[ $# -ne 1 || ! -f "$1" ]]; then
  echo "Usage: bash tools/mac-kpa-avd/test-beta1.sh /path/to/app-release.apk" >&2
  echo "       bash tools/mac-kpa-avd/test-beta1.sh /path/to/ci-artifact.zip" >&2
  exit 2
fi
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
ADB="$SDK/platform-tools/adb"
EXPECTED="Slot_KONKR_API32"
SERIAL="${SLOT_AVD_SERIAL:-}"
TMP=""
cleanup() { if [[ -n "$TMP" ]]; then rm -rf "$TMP"; fi; }
trap cleanup EXIT

if [[ ! -x "$ADB" ]]; then
  echo "Android adb not found: $ADB. Install Android SDK Platform-Tools." >&2
  exit 3
fi
avd_is_slot() {
  "$ADB" -s "$1" emu avd name 2>/dev/null | tr -d '\r' | grep -Fxq "$EXPECTED"
}
if [[ -z "$SERIAL" ]]; then
  while read -r candidate; do
    if avd_is_slot "$candidate"; then SERIAL="$candidate"; break; fi
  done < <("$ADB" devices | awk '$1 ~ /^emulator-[0-9]+$/ && $2 == "device" {print $1}')
fi
if [[ -z "$SERIAL" ]] || ! avd_is_slot "$SERIAL"; then
  echo "The expected $EXPECTED emulator is not running." >&2
  echo "Refusing to install on a KONKR, phone, or another emulator." >&2
  exit 4
fi
ABI="$("$ADB" -s "$SERIAL" shell getprop ro.product.cpu.abi | tr -d '\r')"
if [[ "$ABI" != "arm64-v8a" ]]; then
  echo "AVD ABI is '$ABI'; this APK contains Android ARM64 native cores." >&2
  exit 5
fi

APK="$1"
case "$APK" in
  *.zip)
    TMP="$(mktemp -d)"
    if ! unzip -p "$APK" app-release.apk > "$TMP/app-release.apk"; then
      echo "The ZIP must contain the GitHub Actions 'app-release.apk'." >&2
      exit 6
    fi
    APK="$TMP/app-release.apk"
    ;;
  *.apk) ;;
  *) echo "Expected APK or GitHub Actions ZIP." >&2; exit 7 ;;
esac

echo "Installing on '$EXPECTED' at $SERIAL ($ABI) only."
echo "The existing app data is preserved; the script never uninstalls."
if ! "$ADB" -s "$SERIAL" install -r "$APK"; then
  echo "Install failed. If Android reports a signature mismatch, keep the AVD data" >&2
  echo "and check its existing APK first. This script intentionally will not uninstall." >&2
  exit 8
fi

"$ADB" -s "$SERIAL" shell am start -n fyi.slot.konkr/.MainActivity
echo
echo "Installed version:"
"$ADB" -s "$SERIAL" shell dumpsys package fyi.slot.konkr | grep -m1 -E 'version(Name|Code)=' || true
echo
echo "Mac keyboard: arrows=D-pad, Z=A, X=B, A=X, S=Y, Q/E=L1/R1,"
echo "              1/2=L2/R2, Enter=START, Space=SELECT, Tab=slot. cap."
echo "Use a test ROM copy under the AVD Downloads folder and choose it via START > Library."
echo
echo "When finished, collect diagnostic logs with:"
printf '  "%s" -s "%s" logcat -d -v threadtime -s SlotKonkr:I SlotRomWarm:I SlotAutoLabels:I > ~/Desktop/slot-kpa-beta1-avd.log\n' "$ADB" "$SERIAL"
