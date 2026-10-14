#!/usr/bin/env bash
# A12 read-only GL/Link/PCM logcat capture from both Android peers.
set -euo pipefail
ADB="${ADB:-$HOME/Library/Android/sdk/platform-tools/adb}"
KONKR="${SERIAL_KONKR:-BW0308N250009576}"
MAC="${SERIAL_MAC:-emulator-5556}"
OUT="$HOME/Downloads/slot-kpa-a12-link-diag-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUT"
for pair in "konkr:$KONKR" "mac-avd:$MAC"; do
  label=${pair%%:*}; serial=${pair#*:}
  [[ "$("$ADB" -s "$serial" get-state 2>/dev/null || true)" == device ]] || {
    echo "Disconnected: $label ($serial)" >&2; exit 2; }
  "$ADB" -s "$serial" logcat -d -v threadtime -s 'SlotKONKR:I' 'SlotKonkr:I' '*:S' > "$OUT/$label-tagged.log"
  grep -E 'Link pacing:|Link display pacing:|GL callback pacing:|A12 PCM pacing:|Audio diagnostic:|AudioTrack underruns:|Link fault:|Link transport fault:|Link local GL scheduling gap:|A12 recovery mode:|A12 Link recovery setting:' "$OUT/$label-tagged.log" > "$OUT/$label-summary.log" || true
  grep -q 'GL callback pacing:' "$OUT/$label-summary.log" || echo "No A12 frame records on $label" >&2
done
cat > "$OUT/README.txt" <<'TXT'
A12 read-only capture. GL callback intervals and nativeDrawFrame times do not
include EGL swap/presentation; Link display pacing counts actual 0/1/2
emulated frames per GL callback. PCM metrics are snapshots, not complete
samples of Android HAL buffering. ZIP omits full logs; see local directory.
No ROM or save bytes are collected.
TXT
(cd "$OUT" && zip -q -9 "$OUT.zip" README.txt *-summary.log)
echo "Diagnostic ZIP: $OUT.zip"
