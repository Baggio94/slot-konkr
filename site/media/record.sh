#!/bin/sh
# Re-records the site clips from a copy of the repo's card.
#
#   sh site/media/record.sh CARD [clip...]
#
# CARD is a copy of the repo's sdcard/, never sdcard/ itself: takes write to it. Recharged Yellow
# needs an overworld resume on mGBA and one on gpSP in it. Each take starts from a fresh copy
# with the cart named by the script's `# cart:` line seated (on `# platform:`, gba by default,
# and `# core:` when given),
# or the shelf when it names none. Writes NAME.mp4 and its frame-0 poster NAME.webp beside this
# script. No clip names records all of them.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
card=$(cd "$1" && pwd)
shift
[ $# -gt 0 ] || set -- $(cd "$here/clips" && ls *.txt | sed 's/\.txt$//')
cd "$here/../.."
cargo build -q --release --example record -p slot
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
for name in "$@"; do
	cart=$(sed -n 's/^# cart: *//p' "$here/clips/$name.txt")
	rm -rf "$work/card" && cp -R "$card" "$work/card"
	# The site shows slot on a blank canvas.
	rm -rf "$work/card/Wallpapers"
	platform=$(sed -n 's/^# platform: *//p' "$here/clips/$name.txt")
	[ -n "$cart" ] && [ -z "$platform" ] && platform=gba
	# The stem exactly as the card spells it: a Mac keeps accents decomposed, and slot matches
	# stems byte for byte.
	if [ -n "$cart" ]; then
		cart=$(python3 -c 'import os, sys, unicodedata as u
want = u.normalize("NFC", sys.argv[1])
for f in os.listdir(sys.argv[2]):
    stem = os.path.splitext(f)[0]
    if u.normalize("NFC", stem) == want:
        print(stem)
        break' "$cart" "$card/Games/$(echo "$platform" | tr a-z A-Z)")
	fi
	sed -i.bak "s|^cart=.*|cart=$cart|; s|^cart_platform=.*|cart_platform=$platform|" \
		"$work/card/System/slot.state"
	# `# core: gpsp` starts the seated cart on that core.
	core=$(sed -n 's/^# core: *//p' "$here/clips/$name.txt")
	[ -n "$core" ] && echo "$cart = $core" >> "$work/card/System/selected_core.ini"
	# A script whose `rec` lines name their clips is a session cut into several.
	clips=$(sed -n 's/^rec \([a-z0-9-]*\)$/\1/p' "$here/clips/$name.txt")
	out="$here/$name.mp4"
	[ -n "$clips" ] && out="$here"
	SLOT_SILENT=1 target/release/examples/record "$work/card" "$here/clips/$name.txt" "$out"
	for clip in ${clips:-$name}; do
		ffmpeg -v error -y -i "$here/$clip.mp4" -vf "select=eq(n\,0)" -vframes 1 \
			-c:v libwebp -quality 82 "$here/$clip.webp"
	done
done
