#!/bin/sh
# BaseOS's frontend-session runs this with /bin/sh, ahead of System/slot, and respawns it when
# slot exits. On top of that it runs the power light, points crash capture at the card, and keeps
# slot's log on the card rather than in /tmp.
SD="${SLOT_ROOT:-/mnt/sdcard}"
SYS="$SD/System"
RUN="${AGS_RUN:-/run}"
export SLOT_ROOT="$SD"

log() {
	echo "$(date '+%H:%M:%S') $*" >> "$SD/slot-session.log" 2>/dev/null
}

# One light loop per boot. This script runs again on every respawn, and /run is cleared at boot.
pid="$(cat "$RUN/slot-led.pid" 2>/dev/null)"
if [ -z "$pid" ] || [ ! -d "/proc/$pid" ]; then
	sh "$SYS/led.sh" once
	sh "$SYS/led.sh" loop > /dev/null 2>&1 &
	echo $! > "$RUN/slot-led.pid"
fi

# Set on every start so a pattern something else cleared is put right by the next respawn.
echo "|/bin/sh $SYS/coresave.sh %p %s %e %t" > /proc/sys/kernel/core_pattern 2>/dev/null

# One previous log, so a crash's account survives the respawn that follows it.
[ -f "$SD/slot.log" ] && mv -f "$SD/slot.log" "$SD/slot.log.1"
log "exec $SYS/slot"

# Through the loader: exFAT cannot be trusted to carry an exec bit.
exec /lib/ld-linux-aarch64.so.1 "$SYS/slot" > "$SD/slot.log" 2>&1
