#!/bin/sh
# Builds gpSP's libretro core for the SP from the source archive that ships beside it on the card
# (the GPL-2.0 3(a) source), with every patch beside this script applied.
#
#   build.sh stamp COMMIT                        print what a build of COMMIT would record
#   build.sh build COMMIT TARBALL WORKDIR OUT    build TARBALL into WORKDIR, then OUT and OUT.meta
#
# Upstream's `make platform=arm64` recipe. The CPU goes in through CFLAGS in the environment, which
# the Makefile appends to: CFLAGS on make's command line would silently replace all of its flags.
# The taskfile compares OUT.meta against `stamp`, so a changed pin, flag or patch rebuilds.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"

device_cflags="-mcpu=cortex-a53"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT TARBALL WORKDIR OUT" >&2
	exit 2
}

sha256() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi |
		cut -d' ' -f1
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/gpsp/archive/$1.tar.gz"
	echo "recipe=make platform=arm64"
	echo "device_cflags=$device_cflags"
	for p in "$here"/*.patch; do
		echo "patch=$(basename "$p") sha256:$(sha256 "$p")"
	done
}

build() {
	commit="$1" tarball="$2" work="$3" out="$4"
	src="$work/gpsp-$commit"

	# Unpacked fresh on every run, so nothing from an earlier pin or build is linked in.
	rm -rf "$work"
	mkdir -p "$work"
	tar -xzf "$tarball" -C "$work"
	if [ ! -f "$src/Makefile" ]; then
		echo "$tarball does not hold gpsp-$commit/Makefile" >&2
		exit 1
	fi

	# `stamp` records each patch's sha256, so editing one rebuilds.
	for p in "$here"/*.patch; do
		git -C "$src" apply -p1 "$p"
	done

	# Named explicitly: `git rev-parse` here would find slot's commit, not gpSP's.
	CFLAGS="$device_cflags" make -C "$src" platform=arm64 \
		GIT_VERSION="\"$(printf %s "$commit" | cut -c1-7)\"" \
		-j"$(getconf _NPROCESSORS_ONLN)"

	mkdir -p "$(dirname "$out")"
	cp "$src/gpsp_libretro.so" "$out"
	stamp "$commit" >"$out.meta"
}

case "${1:-}" in
stamp)
	[ $# -eq 2 ] && [ -n "$2" ] || usage
	stamp "$2"
	;;
build)
	[ $# -eq 5 ] && [ -n "$2" ] && [ -n "$3" ] && [ -n "$4" ] && [ -n "$5" ] || usage
	build "$2" "$3" "$4" "$5"
	;;
*)
	usage
	;;
esac
