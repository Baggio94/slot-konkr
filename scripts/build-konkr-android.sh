#!/usr/bin/env bash
# Linux/macOS build of a debug APK; needs Android SDK/NDK and Rust toolchain.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# About uses the Android app versionName rather than a frozen DEV13 label.
SLOT_KONKR_VERSION="$(sed -nE 's/^[[:space:]]*versionName[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' android/app/build.gradle.kts | head -n 1)"
if [ -z "$SLOT_KONKR_VERSION" ]; then
  echo "Cannot read Android versionName" >&2
  exit 1
fi
export SLOT_KONKR_VERSION
rustup target add aarch64-linux-android
if [ ! -f android/app/src/main/jniLibs/arm64-v8a/libmgba_libretro.so ]; then
  bash scripts/build-mgba-android.sh
fi
if [ ! -f android/app/src/main/jniLibs/arm64-v8a/libgpsp_libretro.so ]; then
  bash scripts/build-gpsp-android.sh
fi
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build --release -p slot-android
(cd android && gradle --no-daemon assembleDebug)
echo "APK: android/app/build/outputs/apk/debug/app-debug.apk"
