#!/usr/bin/env bash
# Read-only ADB probe: does NOT alter system or application state.
set -euo pipefail
ADB="$HOME/Library/Android/sdk/platform-tools/adb"
if [[ ! -x "$ADB" ]]; then ADB="$(command -v adb || true)"; fi
if [[ -z "$ADB" ]]; then echo "adb is not installed" >&2; exit 1; fi
"$ADB" devices -l
"$ADB" wait-for-device
echo "--- hardware ---"
for prop in ro.product.manufacturer ro.product.model ro.product.device ro.build.version.release ro.build.version.sdk ro.product.cpu.abi ro.hardware; do
  printf '%-30s %s\n' "$prop" "$("$ADB" shell getprop "$prop" | tr -d '\r')"
done
echo "--- display ---"
"$ADB" shell wm size
"$ADB" shell wm density
echo "--- game controllers ---"
"$ADB" shell dumpsys input | sed -n '1,150p'
echo "Identify actual button keycodes with: adb shell getevent -l"
