#!/usr/bin/env bash
# Linux/macOS build of a debug APK; needs Android SDK/NDK and Rust toolchain.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
rustup target add aarch64-linux-android
if [ ! -f android/app/src/main/jniLibs/arm64-v8a/libmgba_libretro.so ]; then
  bash scripts/build-mgba-android.sh
fi
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build --release -p slot-android
(cd android && gradle --no-daemon assembleDebug)
echo "APK: android/app/build/outputs/apk/debug/app-debug.apk"
