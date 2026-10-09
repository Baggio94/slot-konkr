#!/usr/bin/env bash
# Reproducible libretro gpSP build for Android arm64. No copyrighted BIOS/ROMs.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NDK="${ANDROID_NDK_HOME:-${ANDROID_SDK_ROOT:?Set ANDROID_NDK_HOME or ANDROID_SDK_ROOT}/ndk/27.2.12479018}"
LLVM="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
[ -x "$LLVM/aarch64-linux-android31-clang" ] || {
  echo "Android NDK 27 arm64 compiler not found in $LLVM" >&2; exit 1;
}
# Pinned upstream libretro gpSP commit (2026-09-19).
PIN="5819380c2ffb0900219d700a382ee68c464ebb99"
OUT="$ROOT/.build/gpsp"
SRC="$OUT/source"
mkdir -p "$OUT"
if [ ! -d "$SRC/.git" ]; then
  git init -q "$SRC"
  git -C "$SRC" remote add origin https://github.com/libretro/gpsp.git
fi
if [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null || true)" != "$PIN" ]; then
  git -C "$SRC" fetch --depth=1 origin "$PIN"
  git -C "$SRC" checkout --detach -q FETCH_HEAD
fi
make -C "$SRC" -j4 platform=arm64 \
  CC="$LLVM/aarch64-linux-android31-clang" \
  CXX="$LLVM/aarch64-linux-android31-clang++" \
  AR="$LLVM/llvm-ar"
CORE="$SRC/gpsp_libretro.so"
[ -s "$CORE" ] || { echo "Missing gpSP libretro library" >&2; exit 1; }
DEST="$ROOT/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$DEST"
cp "$CORE" "$DEST/libgpsp_libretro.so"
mkdir -p "$ROOT/android/app/src/main/assets/licenses"
for license in COPYING LICENSE LICENSE.txt; do
  if [ -f "$SRC/$license" ]; then
    cp "$SRC/$license" "$ROOT/android/app/src/main/assets/licenses/gpsp-${license}.txt"
  fi
done
echo "gpSP arm64 core: $DEST/libgpsp_libretro.so"
