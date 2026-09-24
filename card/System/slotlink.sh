#!/bin/sh
# slot's two-device link network, shipped on the card and run as `sh slotlink.sh link <verb>`.
#
#   link host   WPA2 access point on wlan1, addressed 10.42.0.1
#   link join   station on wlan0, addressed 10.42.0.2. Exits 3 when no host answers.
#   link down   tear down whatever a session left
#   link warm   wait for the driver's interfaces, so a later host or join starts faster
#   link cool   accepted and ignored: BaseOS owns the driver
#
# BaseOS loads 8821cs at boot, with its own recovery for radios that reset slowly, so this never
# loads or unloads it. Needs only wpa_supplicant, wpa_cli and busybox, all in BaseOS's rootfs. Every external is
# overridable through AGS_* so the script runs under test off-device.
set -u

NET="${AGS_NET_SYS:-/sys/class/net}"
RUN="${AGS_RUN:-/run}"

# A private two-device network. The PSK keeps the link encrypted, not secret.
LINK_SSID="${AGS_LINK_SSID:-slotlink}"
LINK_PSK="${AGS_LINK_PSK:-slotlink0}"
# Never a DFS channel: on an SP, 5580 MHz kept the AP from ever starting.
LINK_FREQ="${AGS_LINK_FREQ:-5745}"
LINK_HOST_IP="${AGS_LINK_HOST_IP:-10.42.0.1/24}"
LINK_PEER_IP="${AGS_LINK_PEER_IP:-10.42.0.2/24}"
# Waits are in seconds of wall clock, not polls: each poll also runs wpa_cli, about 0.1 s on the
# SP, which stretched a count of 0.25 s polls to half as long again.
LINK_WAIT_S="${AGS_LINK_WAIT_S:-10}"
# How long a joiner stays pinned to LINK_FREQ before scanning every channel. 20 s covers a host
# that starts well after the joiner (its driver and AP take about 6 s); with LINK_WAIT_S behind it
# this matches the 30 s the host listens for.
LINK_PIN_WAIT_S="${AGS_LINK_PIN_WAIT_S:-20}"
# wpa_supplicant rescans only every 5 s on its own, so a joiner asks every 0.5 s.
LINK_RESCAN="${AGS_LINK_RESCAN:-2}"
# Extra global lines for the AP config, for measurements. Empty in production.
LINK_AP_GLOBAL="${AGS_LINK_AP_GLOBAL:-}"
# Append a timeline of each step here (seconds since boot). /dev/kmsg puts it in dmesg.
LINK_TRACE="${AGS_LINK_TRACE:-}"
LINK_MARK="$RUN/slotlink.session"

WPA_SUPPLICANT="${AGS_WPA_SUPPLICANT:-wpa_supplicant}"
WPA_CLI="${AGS_WPA_CLI:-wpa_cli}"
IP="${AGS_IP:-ip}"
UPTIME="${AGS_UPTIME:-/proc/uptime}"
# `wpa_cli -i wlan0` with no -p looks here, so the joiner's socket must live here too.
CTRL_DIR="${AGS_CTRL_DIR:-/var/run/wpa_supplicant}"

trace() {
	[ -n "$LINK_TRACE" ] || return 0
	echo "slotlink: $(cut -d' ' -f1 "$UPTIME" 2>/dev/null) $*" >> "$LINK_TRACE" 2>/dev/null
	return 0
}

# $1 exists under $NET within $2 polls.
wait_dev() {
	w=0
	while [ ! -d "$NET/$1" ] && [ "$w" -lt "$2" ]; do
		sleep 0.25 2>/dev/null || sleep 1
		w=$((w + 1))
	done
	[ -d "$NET/$1" ]
}

# wlan0, which BaseOS brings up in the background at boot: early after boot it may still be
# on its way.
wifi_up() {
	wait_dev wlan0 40 || return 0
	trace "wlan0 present"
	rfkill unblock wifi 2>/dev/null || true
	$IP link set wlan0 up 2>/dev/null || true
}

# Stop the supplicant behind socket $2 in $1 and wait for the socket to go: a new supplicant
# refuses to start while it exists. A socket nobody answers on is removed.
supplicant_stop() {
	if $WPA_CLI -p "$1" -i "$2" terminate >/dev/null 2>&1; then
		s=0
		while [ -e "$1/$2" ] && [ "$s" -lt 8 ]; do
			sleep 0.25 2>/dev/null || sleep 1
			s=$((s + 1))
		done
	elif [ -e "$1/$2" ]; then
		trace "$2: stale socket removed"
		/bin/rm -f "$1/$2"
	fi
	return 0
}

# Everything a session leaves on either end. Run before host and join as well as by down,
# because slot does not always get to call down.
link_clear() {
	supplicant_stop "$RUN/wpa_ap" wlan1
	$IP addr flush dev wlan1 2>/dev/null || true
	/bin/rm -f "$RUN/slotlink-ap.conf"
	if [ -f "$RUN/slotlink-sta.conf" ]; then
		supplicant_stop "$CTRL_DIR" wlan0
		$IP addr flush dev wlan0 2>/dev/null || true
		/bin/rm -f "$RUN/slotlink-sta.conf"
	fi
}

# Wait for COMPLETED. $1 socket dir, $2 interface, $3 bound in seconds, $4 polls between scan
# requests (0 for none), $5 frequency to scan (empty for all). A control-socket SCAN sweeps every
# channel unless told otherwise, which takes seconds.
link_wait() {
	p=0
	was=
	end=$(($(date +%s) + $3))
	while :; do
		state=$($WPA_CLI -p "$1" -i "$2" status 2>/dev/null | sed -n 's/^wpa_state=//p')
		if [ "$state" != "$was" ]; then
			trace "$2 ${state:-no answer}"
			was=$state
		fi
		[ "$state" = COMPLETED ] && return 0
		[ "$(date +%s)" -ge "$end" ] && return 1
		if [ "$4" -gt 0 ] && [ "$p" -gt 0 ] && [ $((p % $4)) -eq 0 ]; then
			case "$state" in
			DISCONNECTED | INACTIVE | SCANNING)
				$WPA_CLI -p "$1" -i "$2" scan ${5:+freq=$5} >/dev/null 2>&1
				;;
			esac
		fi
		sleep 0.25 2>/dev/null || sleep 1
		p=$((p + 1))
	done
}

# The host always uses wlan1, which the driver registers beside wlan0.
link_host() {
	echo "$$" > "$LINK_MARK"
	trace "host: asked"
	link_clear
	wifi_up
	wait_dev wlan1 4 || return 1
	command -v "$WPA_SUPPLICANT" >/dev/null 2>&1 || return 1

	/bin/mkdir -p "$RUN/wpa_ap"
	{
		echo "ctrl_interface=$RUN/wpa_ap"
		# Start the AP without first scanning for a network to join: 0.4 s instead of 4.5 s.
		echo 'ap_scan=2'
		if [ -n "$LINK_AP_GLOBAL" ]; then
			printf '%s\n' "$LINK_AP_GLOBAL"
		fi
		echo 'network={'
		echo "	ssid=\"$LINK_SSID\""
		echo '	mode=2'
		echo "	frequency=$LINK_FREQ"
		echo '	key_mgmt=WPA-PSK'
		echo '	proto=RSN'
		echo '	pairwise=CCMP'
		echo "	psk=\"$LINK_PSK\""
		echo '}'
	} > "$RUN/slotlink-ap.conf"
	/bin/chmod 600 "$RUN/slotlink-ap.conf"

	$IP link set wlan1 up 2>/dev/null || true
	$WPA_SUPPLICANT -B -i wlan1 -c "$RUN/slotlink-ap.conf" -Dnl80211 >/dev/null 2>&1 || return 1
	# Never address an AP that did not come up: that looks like a working link with nobody there.
	link_wait "$RUN/wpa_ap" wlan1 "$LINK_WAIT_S" 0 "" || return 1
	$IP addr add "$LINK_HOST_IP" dev wlan1 2>/dev/null || true
	trace "host: addressed"
}

# $1 is the frequency to pin, or empty to scan everything. Pinned associates in about 1 s.
link_sta_conf() {
	{
		echo "ctrl_interface=$CTRL_DIR"
		echo 'network={'
		echo "	ssid=\"$LINK_SSID\""
		echo '	key_mgmt=WPA-PSK'
		echo "	psk=\"$LINK_PSK\""
		if [ -n "${1:-}" ]; then
			echo "	scan_freq=$1"
			echo "	freq_list=$1"
		fi
		echo '}'
	} > "$RUN/slotlink-sta.conf"
	/bin/chmod 600 "$RUN/slotlink-sta.conf"
}

# Attempt one pins LINK_FREQ; attempt two scans every channel, for a host that is also on a home
# network and so sits on that network's channel.
link_join() {
	echo "$$" > "$LINK_MARK"
	trace "join: asked"
	link_clear
	wifi_up
	[ -d "$NET/wlan0" ] || return 1
	/bin/mkdir -p "$CTRL_DIR"
	$IP link set wlan0 up 2>/dev/null || true

	tries="${AGS_LINK_TRIES:-2}"
	attempt=1
	while [ "$attempt" -le "$tries" ]; do
		if [ "$attempt" -eq 1 ]; then
			freq="$LINK_FREQ"
			bound="$LINK_PIN_WAIT_S"
		else
			freq=
			bound="$LINK_WAIT_S"
		fi
		link_sta_conf "$freq"
		supplicant_stop "$CTRL_DIR" wlan0
		$WPA_SUPPLICANT -B -i wlan0 -c "$RUN/slotlink-sta.conf" -Dnl80211 >/dev/null 2>&1 \
			|| { attempt=$((attempt + 1)); continue; }
		trace "join: attempt $attempt on ${freq:-every channel}"
		if link_wait "$CTRL_DIR" wlan0 "$bound" "$LINK_RESCAN" "$freq"; then
			$IP addr add "$LINK_PEER_IP" dev wlan0 2>/dev/null || true
			trace "join: addressed"
			return 0
		fi
		attempt=$((attempt + 1))
	done
	trace "join: no host found"
	# 3 means the radio worked and nobody answered; 1 means the radio failed.
	return 3
}

link_down() {
	trace "down"
	link_clear
	$IP link set wlan1 down 2>/dev/null || true
	rmdir "$RUN/wpa_ap" 2>/dev/null || true
	/bin/rm -f "$LINK_MARK"
	return 0
}

link_warm() {
	trace "warm"
	wifi_up
	[ -d "$NET/wlan0" ] || return 1
	wait_dev wlan1 4
}

case "${1:-} ${2:-}" in
"link host") link_host ;;
"link join") link_join ;;
"link down") link_down ;;
"link warm") link_warm ;;
"link cool") exit 0 ;;
*)
	echo "usage: slotlink.sh link [host|join|down|warm|cool]" >&2
	exit 2
	;;
esac
