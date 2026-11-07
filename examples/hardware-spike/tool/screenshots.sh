#!/usr/bin/env bash
# Captures the store screenshots on a running simulator/emulator.
#   tool/screenshots.sh <device-id> <label>      e.g. tool/screenshots.sh 627EB8F2-... iphone-6.9
# Output: build/screenshots/<label>/*.png
set -euo pipefail
cd "$(dirname "$0")/.."
device="${1:?usage: tool/screenshots.sh <device-id> <label>}"
label="${2:?usage: tool/screenshots.sh <device-id> <label>}"
SHOTS_DIR="$PWD/build/screenshots/$label" flutter drive \
  --driver=test_driver/screenshots.dart \
  --target=integration_test/screenshots_test.dart \
  -d "$device" --dart-define=SOURCE_REVISION="$(git rev-parse HEAD)"
# The stores reject screenshots with an alpha channel: flatten to JPEG.
for png in "build/screenshots/$label"/*.png; do
  sips -s format jpeg -s formatOptions best "$png" --out "${png%.png}.jpg" >/dev/null
  rm "$png"
done
# Google Play: the long side may be at most twice the short side; trim the bottom of taller shots.
for jpg in "build/screenshots/$label"/*.jpg; do
  case "$label" in android-*) ;; *) break ;; esac
  w=$(sips -g pixelWidth "$jpg" | awk '/pixelWidth/{print $2}')
  h=$(sips -g pixelHeight "$jpg" | awk '/pixelHeight/{print $2}')
  if [ "$h" -gt $((w * 2)) ]; then
    sips -c $((w * 2)) "$w" --cropOffset 0 0 "$jpg" >/dev/null
  fi
done
ls -l "build/screenshots/$label"
