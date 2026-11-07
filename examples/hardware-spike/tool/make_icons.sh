#!/usr/bin/env bash
# Resizes assets/icon/icon-source.png into the iOS AppIcon set, the Android launcher mipmaps and
# a 512 px Play Store icon (assets/icon/play-store-512.png). The source must have no alpha.
set -euo pipefail
cd "$(dirname "$0")/.."
src=assets/icon/icon-source.png

ios=ios/Runner/Assets.xcassets/AppIcon.appiconset
python3 - "$src" "$ios" <<'PY'
import json, subprocess, sys
src, ios = sys.argv[1:]
seen = set()
for img in json.load(open(f"{ios}/Contents.json"))["images"]:
    name = img["filename"]
    if name in seen:
        continue
    seen.add(name)
    px = round(float(img["size"].split("x")[0]) * int(img["scale"].rstrip("x")))
    subprocess.run(["sips", "-z", str(px), str(px), src, "--out", f"{ios}/{name}"],
                   check=True, stdout=subprocess.DEVNULL)
PY

res=android/app/src/main/res
for pair in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  d=${pair%%:*}; px=${pair##*:}
  sips -z "$px" "$px" "$src" --out "$res/mipmap-$d/ic_launcher.png" >/dev/null
done
sips -z 512 512 "$src" --out assets/icon/play-store-512.png >/dev/null
echo "icons written"
