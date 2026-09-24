#!/bin/sh
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/System/coresave.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

# The kernel hands this script a core on stdin and nothing else, so every case here pipes one
# in. The card, the mount check and the crashing process's path are injected, which is what
# lets this run on a machine with no card, no /proc and no root.
setup() {
	rm -rf "$TMP/card" "$TMP/mountpoint"
	mkdir -p "$TMP/card"
	cat > "$TMP/mountpoint" <<-SH
		#!/bin/sh
		exit ${1:-0}
	SH
	chmod +x "$TMP/mountpoint"
}

run() {
	# $1 the crashing process's path, $2 the core's contents, then pid, signal and thread.
	printf '%s' "$2" | AGS_CARD="$TMP/card" AGS_MOUNTPOINT="$TMP/mountpoint" \
		AGS_CORE_EXE="$1" sh "$SCRIPT" "${3:-4321}" "${4:-11}" "${5:-slot-emu}" 0
}

# A crash that is not the frontend's still earns its line: which process died, on what signal,
# and in which thread is the whole of what a silent death otherwise leaves behind.
setup
run /usr/bin/busybox "not a frontend core"
grep -q 'signal=11 thread=slot-emu exe=/usr/bin/busybox' "$TMP/card/crash/cores.log" \
	|| { echo "no log line for a non-frontend crash: [$(cat "$TMP/card/crash/cores.log")]" >&2; exit 1; }
[ ! -f "$TMP/card/crash/slot.core" ] \
	|| { echo "kept a core for a process that is not the frontend" >&2; exit 1; }

# The frontend's core is the one worth the card space, because it is the one nobody can
# reproduce on a desk.
setup
run /mnt/sdcard/System/slot "first core"
[ "$(cat "$TMP/card/crash/slot.core")" = "first core" ] \
	|| { echo "the frontend's core was not saved" >&2; exit 1; }

# Only the latest, and under one name: slot's core is about 150 MB, so a card that kept every
# one would fill over a few bad days. A half-written core is never left in its place either:
# the write lands beside it and is moved in.
run /mnt/sdcard/System/slot "second core"
[ "$(cat "$TMP/card/crash/slot.core")" = "second core" ] \
	|| { echo "a second crash did not replace the core" >&2; exit 1; }
[ ! -f "$TMP/card/crash/slot.core.tmp" ] \
	|| { echo "left a half-written core behind" >&2; exit 1; }
[ "$(grep -c . "$TMP/card/crash/cores.log")" = "2" ] \
	|| { echo "cores.log did not record both crashes" >&2; exit 1; }

# No card, no evidence, but the pipe still has to be drained and the exit still has to be
# clean, or the kernel holds the dying process while the frontend is already being respawned.
setup
cat > "$TMP/mountpoint" <<-SH
	#!/bin/sh
	exit 1
SH
chmod +x "$TMP/mountpoint"
run /mnt/sdcard/System/slot "core with nowhere to go" \
	|| { echo "a missing card made the helper fail" >&2; exit 1; }
[ ! -d "$TMP/card/crash" ] \
	|| { echo "wrote to a card that is not mounted" >&2; exit 1; }

echo "coresave: all passed"
