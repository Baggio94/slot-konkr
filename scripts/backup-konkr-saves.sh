#!/usr/bin/env bash
# Read-only export of Slot KONKR private saves from a debuggable preview APK.
set -euo pipefail
ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
SER="${1:-BW0308N250009576}"
DEST="${2:-$HOME/Downloads}"
APP="fyi.slot.konkr"
test -x "$ADB" || { echo "ADB not found" >&2; exit 1; }
mkdir -p "$DEST"
OUT="$DEST/Slot-KONKR-Saves-$(date +%Y%m%d-%H%M%S).tar"
TMP="$OUT.tmp"
trap 'rm -f "$TMP"' EXIT
"$ADB" -s "$SER" get-state | grep -qx device
"$ADB" -s "$SER" shell run-as "$APP" ls files/Saves >/dev/null
"$ADB" -s "$SER" exec-out run-as "$APP" sh -c 'cd files && tar -cf - Saves' > "$TMP"
test -s "$TMP" || { echo "Empty save archive" >&2; exit 1; }
tar -tf "$TMP" >/dev/null || { echo "Invalid TAR archive" >&2; exit 1; }
mv "$TMP" "$OUT"
trap - EXIT
echo "Verified backup: $OUT"
