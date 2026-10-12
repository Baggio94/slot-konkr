#!/usr/bin/env bash
# Prepare an isolated Android 12 KPA-like AVD without touching SleepSync AVDs.
set -euo pipefail
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
EMULATOR="$SDK/emulator/emulator"
MANAGER="$SDK/cmdline-tools/latest/bin/avdmanager"
PACKAGES="$SDK/cmdline-tools/latest/bin/sdkmanager"
ADB="$SDK/platform-tools/adb"
NAME="Slot_KONKR_API32"

for path in "$EMULATOR" "$MANAGER" "$PACKAGES" "$ADB"; do
  if [[ ! -x "$path" ]]; then
    echo "Android SDK component missing: $path" >&2
    echo 'Install Android Studio, Android Emulator and Android SDK Command-line Tools (latest).' >&2
    exit 2
  fi
done

if [[ "$(uname -m)" != arm64 ]]; then
  echo 'This APK contains Android arm64 native mGBA/gpSP libraries.' >&2
  echo 'This setup is for Apple Silicon Macs (arm64); Intel Mac requires building x86_64 Android libs.' >&2
  exit 3
fi

IMAGE='system-images;android-32;google_apis;arm64-v8a'
"$PACKAGES" --install 'platforms;android-32' "$IMAGE"
if ! "$EMULATOR" -list-avds | grep -Fxq "$NAME"; then
  echo no | "$MANAGER" create avd -n "$NAME" -k "$IMAGE" -d pixel --force
fi
INI="$HOME/.android/avd/$NAME.avd/config.ini"
if [[ ! -f "$INI" ]]; then echo "AVD config not found: $INI" >&2; exit 4; fi
cp -n "$INI" "$INI.before-slot" 2>/dev/null || true
python3 - "$INI" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]); contents=p.read_text().splitlines(); d={}
for line in contents:
    if '=' in line and not line.lstrip().startswith('#'):
        k,v=line.split('=',1); d[k.strip()]=v.strip()
d.update({
    'hw.lcd.width':'960','hw.lcd.height':'640',
    'hw.lcd.density':'240','hw.keyboard':'yes','hw.ramSize':'4096',
    'hw.gpu.enabled':'yes','hw.gpu.mode':'auto',
    'hw.mainKeys':'yes','hw.dPad':'yes',
    'hw.initialOrientation':'landscape',
    'showDeviceFrame':'no',
    'avd.ini.displayname':'Slot KONKR Pocket Advance (Android 12)',
})
p.write_text('\n'.join(f'{key}={value}' for key,value in d.items())+'\n')
PY
printf 'AVD %s ready (Android 12/API32; 960x640 3:2).\n' "$NAME"
echo "Launch: $EMULATOR -avd $NAME -gpu auto -no-boot-anim"
echo 'Once booted, run configure.sh [path/to/slot.apk] to install Slot and map the landscape display.'
