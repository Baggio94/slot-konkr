#!/usr/bin/env bash
# Build a reproducible arm64 mGBA libretro binary for the Android KONKR shell.
# Requires git, cmake, Android NDK 27+; no commercial ROMs or BIOS files.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -n "$ANDROID_NDK_HOME" ]; then
  NDK="$ANDROID_NDK_HOME"
else
  NDK="$ANDROID_SDK_ROOT/ndk/27.2.12479018"
fi
[ -f "$NDK/build/cmake/android.toolchain.cmake" ] || { echo "Set ANDROID_NDK_HOME to your installed NDK" >&2; exit 1; }
PIN="7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6"
OUT="$ROOT/.build/mgba"
SRC="$OUT/source"
BUILD="$OUT/android-arm64"
mkdir -p "$OUT"
if [ ! -d "$SRC/.git" ]; then
  git init -q "$SRC"
  git -C "$SRC" remote add origin https://github.com/libretro/mgba.git
fi
if [ "$(git -C "$SRC" rev-parse HEAD 2>/dev/null || true)" != "$PIN" ]; then
  git -C "$SRC" fetch --depth=1 origin "$PIN"
  git -C "$SRC" checkout --detach -q FETCH_HEAD
fi
cmake -S "$SRC" -B "$BUILD" \
  -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-31 \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_LIBRETRO=ON -DSKIP_LIBRARY=ON \
  -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_GL=OFF \
  -DBUILD_GLES2=OFF -DBUILD_GLES3=OFF \
  -DBUILD_SHARED=OFF -DBUILD_STATIC=OFF \
  -DUSE_FFMPEG=OFF -DUSE_PNG=OFF -DUSE_SQLITE3=OFF \
  -DUSE_LUA=OFF -DUSE_JSON_C=OFF -DUSE_LIBZIP=OFF \
  -DUSE_LZMA=OFF -DENABLE_SCRIPTING=OFF \
  -DBUILD_LTO=OFF -DENABLE_DEBUGGERS=OFF \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON
cmake --build "$BUILD" --target mgba_libretro -j 4
CORE="$(find "$BUILD" -type f -name '*libretro*.so' -print -quit)"
[ -n "$CORE" ] && [ -s "$CORE" ] || { echo "mGBA libretro core missing" >&2; exit 1; }
DEST="$ROOT/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$DEST"
cp "$CORE" "$DEST/libmgba_libretro.so"
mkdir -p "$ROOT/android/app/src/main/assets/licenses"
if [ -f "$SRC/LICENSE" ]; then
  cp "$SRC/LICENSE" "$ROOT/android/app/src/main/assets/licenses/mgba-LICENSE.txt"
fi
echo "mGBA arm64 libretro: $DEST/libmgba_libretro.so"
