#!/usr/bin/env bash
# All store screenshots, one device after another (they share one build directory).
#   iOS:     iPhone 6.9" (App Store), iPad 13" (App Store)
#   Android: phone, 7" tablet, 10" tablet (AVDs created with avdmanager: spike-phone,
#            spike-tablet7, spike-tablet10)
# Optional argument: ios or android, to run only that platform.
# Output: build/screenshots/<label>/*.png
set -euo pipefail
cd "$(dirname "$0")/.."
sdk="${ANDROID_HOME:-$HOME/Library/Android/sdk}"

sim_udid() { # newest runtime's simulator with exactly this name
  xcrun simctl list devices available -j | python3 -c '
import json, sys
name = sys.argv[1]
devices = json.load(sys.stdin)["devices"]
print(next(d["udid"] for rt, ds in sorted(devices.items(), reverse=True) for d in ds if d["name"] == name))' "$1"
}

ios() { # <simulator name> <label>
  local udid; udid=$(sim_udid "$1")
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  tool/screenshots.sh "$udid" "$2"
  xcrun simctl shutdown "$udid"
}

android() { # <avd name> <label>
  "$sdk/emulator/emulator" -avd "$1" -no-snapshot -no-audio -no-boot-anim -gpu host >/dev/null 2>&1 &
  local pid=$!
  "$sdk/platform-tools/adb" wait-for-device
  until [ "$("$sdk/platform-tools/adb" shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done
  local serial; serial=$("$sdk/platform-tools/adb" devices | awk '/^emulator-/{print $1; exit}')
  tool/screenshots.sh "$serial" "$2" || { kill "$pid"; return 1; }
  "$sdk/platform-tools/adb" -s "$serial" emu kill >/dev/null || kill "$pid"
  wait "$pid" 2>/dev/null || true
}

only="${1:-all}"
if [ "$only" != android ]; then
  ios "iPhone 18 Pro Max" iphone-6.9
  ios "iPad Pro 13-inch (M5)" ipad-13
fi
if [ "$only" != ios ]; then
  android spike-phone android-phone
  android spike-tablet10 android-tablet-10
  android spike-tablet7 android-tablet-7
fi
