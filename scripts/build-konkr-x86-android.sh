#!/usr/bin/env bash
# Build native x86_64 Android counterparts for the Mac AVD without altering
# the pinned arm64 KONKR binaries. One APK must carry all three libraries for
# BOTH ABIs, otherwise "same APK" Host/Join testing is impossible.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
NDK="${ANDROID_NDK_HOME:-${ANDROID_SDK_ROOT:-${ANDROID_HOME:?Set ANDROID_NDK_HOME or ANDROID_SDK_ROOT}}/ndk/27.2.12479018}"
case "$(uname -s)" in
  Linux) HOST=linux-x86_64 ;;
  Darwin) HOST=darwin-x86_64 ;;
  *) echo "Unsupported host" >&2; exit 1 ;;
esac
LLVM="$NDK/toolchains/llvm/prebuilt/$HOST/bin"
CC="$LLVM/x86_64-linux-android31-clang"
CXX="$LLVM/x86_64-linux-android31-clang++"
NM="$LLVM/llvm-nm"
test -x "$CC" && test -x "$NM" || { echo "Android NDK x86_64 toolchain missing" >&2; exit 1; }
DEST="$ROOT/android/app/src/main/jniLibs/x86_64"
mkdir -p "$DEST"

# mGBA: reuse the exact seven patches + speculative-audio sidecar already
# applied to the pinned SOURCE by build-mgba-android.sh (arm64 step in CI).
M_SRC="$ROOT/.build/mgba/source"
test -d "$M_SRC/.git" || { echo "Build arm64 mGBA first (pinned source/patches)" >&2; exit 1; }
M_BUILD="$ROOT/.build/mgba/android-x86_64"
cmake -S "$M_SRC" -B "$M_BUILD" \
  -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=x86_64 -DANDROID_PLATFORM=android-31 \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
  -DBUILD_LIBRETRO=ON -DSKIP_LIBRARY=ON \
  -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_GL=OFF \
  -DBUILD_GLES2=OFF -DBUILD_GLES3=OFF \
  -DBUILD_SHARED=OFF -DBUILD_STATIC=OFF \
  -DUSE_FFMPEG=OFF -DUSE_PNG=OFF -DUSE_SQLITE3=OFF \
  -DUSE_LUA=OFF -DUSE_JSON_C=OFF -DUSE_LIBZIP=OFF \
  -DUSE_LZMA=OFF -DENABLE_SCRIPTING=OFF \
  -DBUILD_LTO=OFF -DENABLE_DEBUGGERS=OFF \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON
cmake --build "$M_BUILD" --target mgba_libretro -j 4
MGBA="$(find "$M_BUILD" -type f -name '*libretro*.so' -print -quit)"
test -n "$MGBA" && test -s "$MGBA" || { echo "Missing x86_64 mGBA core" >&2; exit 1; }
cp "$MGBA" "$DEST/libmgba_libretro.so"
for symbol in retro_init retro_load_game retro_run retro_serialize retro_unserialize slot_mgba_audio_sidecar slot_mgba_audio_sidecar_size; do
  "$NM" -D --defined-only "$DEST/libmgba_libretro.so" | grep -q " $symbol$" || { echo "mGBA x86_64 symbol missing: $symbol" >&2; exit 1; }
done

# gpSP: build a clean object tree for x86_64 instead of reusing compiled
# arm64 objects. Use the portable interpreter: never silently link an
# architecture-inappropriate 32-bit x86 dynarec into a 64-bit Android core.
PIN=5819380c2ffb0900219d700a382ee68c464ebb99
G_SRC="$ROOT/.build/gpsp-x86_64/source"
if [ ! -d "$G_SRC/.git" ]; then
  mkdir -p "$G_SRC"
  git init -q "$G_SRC"
  git -C "$G_SRC" remote add origin https://github.com/libretro/gpsp.git
fi
if [ "$(git -C "$G_SRC" rev-parse HEAD 2>/dev/null || true)" != "$PIN" ]; then
  git -C "$G_SRC" fetch --depth=1 origin "$PIN"
  git -C "$G_SRC" checkout --detach -q FETCH_HEAD
fi
python3 "$ROOT/scripts/patch-gpsp-speculative-audio.py" "$G_SRC"
make -C "$G_SRC" -j4 platform=unix HAVE_DYNAREC=0 MMAP_JIT_CACHE=0 \
  CC="$CC" CXX="$CXX" AR="$LLVM/llvm-ar"
test -s "$G_SRC/gpsp_libretro.so" || { echo "Missing x86_64 gpSP core" >&2; exit 1; }
cp "$G_SRC/gpsp_libretro.so" "$DEST/libgpsp_libretro.so"
for symbol in retro_init retro_load_game retro_run retro_serialize retro_unserialize slot_gpsp_audio_sidecar slot_gpsp_audio_sidecar_size; do
  "$NM" -D --defined-only "$DEST/libgpsp_libretro.so" | grep -q " $symbol$" || { echo "gpSP x86_64 symbol missing: $symbol" >&2; exit 1; }
done
echo "x86_64 mGBA + gpSP core builds complete; JNI Rust is staged separately"
