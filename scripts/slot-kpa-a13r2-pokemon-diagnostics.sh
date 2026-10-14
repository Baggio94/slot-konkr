#!/usr/bin/env bash
# Read-only KMLB/4 stage trace (no ROM/SRAM or private game files).
set -euo pipefail
ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
KONKR="${SERIAL_KONKR:-BW0308N250009576}"
MAC="${SERIAL_MAC:-emulator-5556}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="$HOME/Downloads/slot-kpa-a13r2-pokemon-diag-$STAMP"
mkdir -p "$OUT"
for pair in "konkr:$KONKR" "mac-avd:$MAC"; do
  label="${pair%%:*}"; serial="${pair#*:}"
  if [[ "$("$ADB" -s "$serial" get-state 2>/dev/null || true)" != device ]]; then
    echo "ADB offline: $serial" >&2; exit 2
  fi
  "$ADB" -s "$serial" logcat -d -v threadtime -s 'SlotKONKR:I' 'SlotKonkr:I' '*:S' |
    grep -E 'KMLB4 ROM exchange:|KMLB4 setup failure:|Link transport fault:|Mac bridge found Slot Link host|mixed-ROM transfer preauthorized|Multiplayer:|Link -> SOLO' \
    > "$OUT/$label-link-trace.log" || true
done
cat > "$OUT/README.txt" <<'TXT'
Read-only A13r2 KMLB4 transport phase diagnostic.
Contains timestamps, ROM sizes and metadata-only errors, NO ROM data,
save data, passwords or peer SHA256 values. No device data modified.
TXT
(cd "$OUT" && zip -q -9 "$OUT.zip" README.txt *.log)
echo "KMLB4 diagnostic ZIP: $OUT.zip"
