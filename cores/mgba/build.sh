#!/bin/sh
# Builds mGBA's libretro core from libretro/mgba at a pinned commit with every patch beside this
# script applied, using libretro CI's flags.
#
#   build.sh stamp COMMIT               print what a build of COMMIT would record
#   build.sh build COMMIT WORKDIR OUT   build into WORKDIR, then write OUT and OUT.meta
#
# The taskfile compares OUT.meta against `stamp`, so a changed pin, patch or flag rebuilds.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"

# LTO for the device core only: 3 to 5% off every frame on the SP. -mcpu=cortex-a53 cancelled the
# gain. Never add anything that changes computed values (no -ffast-math): linked SPs must stay bit
# identical. `stamp` prints it on every host because the Mac checks the device .meta.
device_cflags="-flto=auto"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT WORKDIR OUT" >&2
	exit 2
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
		cut -d' ' -f1
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/mgba/tree/$1"
	echo "device_cflags=$device_cflags"
	for p in "$here"/*.patch; do
		echo "patch=$(basename "$p") sha256:$(sha256 "$p")"
	done
}

build() {
	commit="$1" work="$2" out="$3"
	src="$work/mgba"
	obj="$work/build"

	if ! command -v cmake >/dev/null 2>&1; then
		if command -v apt-get >/dev/null 2>&1; then
			# Bullseye is out of security support: its Release files are expired and
			# bullseye-security 404s some packages, so use the main archive only.
			sed -i '/bullseye-security/d' /etc/apt/sources.list
			apt-get -o Acquire::Check-Valid-Until=false update -qq
			apt-get install -y -qq --no-install-recommends cmake >/dev/null
		else
			echo "building mGBA needs cmake: brew install cmake" >&2
			exit 1
		fi
	fi

	# Pristine checkout every run so patches never stack. Its own .git is created first so git
	# cannot fall through to slot's repository.
	mkdir -p "$src"
	[ -d "$src/.git" ] || git init -q "$src"
	git -C "$src" cat-file -e "$commit^{commit}" 2>/dev/null ||
		git -C "$src" fetch -q --depth 1 https://github.com/libretro/mgba "$commit"
	git -C "$src" checkout -q --force --detach "$commit"
	git -C "$src" clean -q -fdx
	for p in "$here"/*.patch; do
		git -C "$src" apply "$p"
	done

	# Passed even when empty so a reused build tree cannot keep stale flags.
	cflags=""
	if [ "$(uname -s)-$(uname -m)" = "Linux-aarch64" ]; then
		cflags="$device_cflags"
	fi
	cmake -S "$src" -B "$obj" -DLIBMGBA_ONLY=ON -DBUILD_LIBRETRO=ON -DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_C_FLAGS="$cflags" >/dev/null
	cmake --build "$obj" --target mgba_libretro --parallel "$(getconf _NPROCESSORS_ONLN)" >/dev/null

	for ext in dylib so; do
		if [ -f "$obj/mgba_libretro.$ext" ]; then
			mkdir -p "$(dirname "$out")"
			cp "$obj/mgba_libretro.$ext" "$out"
			stamp "$commit" >"$out.meta"
			return
		fi
	done
	echo "cmake finished without producing mgba_libretro" >&2
	exit 1
}

case "${1:-}" in
stamp)
	[ $# -eq 2 ] && [ -n "$2" ] || usage
	stamp "$2"
	;;
build)
	[ $# -eq 4 ] && [ -n "$2" ] && [ -n "$3" ] && [ -n "$4" ] || usage
	build "$2" "$3" "$4"
	;;
*)
	usage
	;;
esac
