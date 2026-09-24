#!/bin/sh
# The power light, run as `sh led.sh [off|once|loop]`. launch_frontend.sh starts one loop per boot.
#
# Two GPIO LEDs on the battery node, both write-only: work_led is green, lowpwr_led is red. The
# orange charge light is the PMIC's own CHGLED pin and needs nothing from here.
set -u

PSY="${AGS_PSY:-/sys/class/power_supply/axp2202-battery}"
STATE="${AGS_LED_STATE:-/run/slot-led.state}"
INTERVAL="${AGS_LED_INTERVAL:-30}"

# Red at or below LOW and green again only at CLEAR: the gauge moves in whole percent, so one
# threshold would strobe.
LOW="${AGS_LED_LOW:-15}"
CLEAR="${AGS_LED_CLEAR:-20}"

read_or() {
	if [ -r "$1" ]; then
		cat "$1" 2>/dev/null
	else
		echo "$2"
	fi
}

green() {
	echo 1 > "$PSY/work_led" 2>/dev/null
	echo 0 > "$PSY/lowpwr_led" 2>/dev/null
}

red() {
	echo 0 > "$PSY/work_led" 2>/dev/null
	echo 1 > "$PSY/lowpwr_led" 2>/dev/null
}

dark() {
	echo 0 > "$PSY/work_led" 2>/dev/null
	echo 0 > "$PSY/lowpwr_led" 2>/dev/null
}

# The colour last shown. `off` leaves it alone so the next `once` restores it.
last() { read_or "$STATE" green; }

apply() {
	case "$1" in
	red) red ;;
	*) green ;;
	esac
	echo "$1" > "$STATE" 2>/dev/null
}

# The colour the reading calls for, given the colour already showing ($1).
want() {
	was="$1"
	cap="$(read_or "$PSY/capacity" "")"
	status="$(read_or "$PSY/status" "")"
	# Never red while charging: the orange light already says what is happening.
	case "$status" in
	Charging | Full)
		echo green
		return
		;;
	esac
	# No gauge, or one that will not parse, keeps the current colour.
	case "$cap" in
	'' | *[!0-9]*)
		echo "$was"
		return
		;;
	esac
	if [ "$was" = red ]; then
		if [ "$cap" -ge "$CLEAR" ]; then echo green; else echo red; fi
	else
		if [ "$cap" -le "$LOW" ]; then echo red; else echo green; fi
	fi
}

case "${1:-loop}" in
off)
	dark
	;;
once)
	apply "$(want "$(last)")"
	;;
loop)
	# Re-asserted every tick: the attributes are write-only, so a light something else turned
	# off can only be healed by writing it again.
	while :; do
		apply "$(want "$(last)")"
		sleep "$INTERVAL"
	done
	;;
*)
	echo "usage: led.sh [off|once|loop]" >&2
	exit 2
	;;
esac
