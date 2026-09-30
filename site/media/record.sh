#!/bin/sh
# Re-records the site clips from a copy of an SP's card.
#
#   sh site/media/record.sh CARD [clip...]
#
# CARD is a folder holding a card's Games, States, Saves, Labels, BIOS, Wallpapers and System
# (`adb pull` them off the SP). Each take starts from a fresh copy of it with the cart named by
# the script's `# cart:` line seated, or the shelf when it names none. Writes NAME.mp4 and its
# frame-0 poster NAME.webp beside this script. No clip names records all of them.
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
	platform=""
	[ -n "$cart" ] && platform=gba
	sed -i.bak "s|^cart=.*|cart=$cart|; s|^cart_platform=.*|cart_platform=$platform|" \
		"$work/card/System/slot.state"
	SLOT_SILENT=1 target/release/examples/record "$work/card" "$here/clips/$name.txt" \
		"$here/$name.mp4"
	ffmpeg -v error -y -i "$here/$name.mp4" -vf "select=eq(n\,0)" -vframes 1 \
		-c:v libwebp -quality 82 "$here/$name.webp"
done
