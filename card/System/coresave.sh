#!/bin/sh
# Keeps the evidence when a process crashes. launch_frontend.sh points core_pattern here:
#
#   |/bin/sh <card>/System/coresave.sh %p %s %e %t    (pid, signal, thread, unix time)
#
# A pipe because slot inherits a core limit of 0 from init, which the kernel does not apply to a
# pipe, and this kernel logs no user-space faults. Every crash gets a line in crash/cores.log;
# only slot's latest core is kept, since each is about 150 MB.
SD="${AGS_CARD:-/mnt/sdcard}"
MOUNTPOINT="${AGS_MOUNTPOINT:-mountpoint}"
DIR="$SD/crash"

# The pipe must be drained whatever happens, or the dying process is held.
if ! $MOUNTPOINT -q "$SD" 2>/dev/null; then
	cat > /dev/null
	exit 0
fi

exe="${AGS_CORE_EXE:-$(readlink "/proc/$1/exe" 2>/dev/null)}"
mkdir -p "$DIR" 2>/dev/null
echo "$(date '+%F %T') pid=$1 signal=$2 thread=$3 exe=$exe" >> "$DIR/cores.log"

case "$exe" in
*/slot)
	# The maps say where the cores' dylibs were loaded, which reading the core elsewhere needs.
	cp "/proc/$1/maps" "$DIR/slot.maps" 2>/dev/null
	cat > "$DIR/slot.core.tmp" && mv -f "$DIR/slot.core.tmp" "$DIR/slot.core"
	;;
*)
	cat > /dev/null
	;;
esac
sync
