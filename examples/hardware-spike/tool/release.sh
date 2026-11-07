#!/usr/bin/env bash
# Builds the hardware spike for the stores (internal testing).
#
#   tool/release.sh android   -> build/app/outputs/bundle/release/app-release.aab
#   tool/release.sh ios       -> build/ios/ipa/*.ipa, uploaded to TestFlight when the ASC key is set
#   tool/release.sh both
#
# Secrets live in ~/.keys/hardware-spike/ (override with KEYS_DIR):
#   key.properties, upload-keystore.jks   made by tool/setup_android_key.sh
#   ios.env                               APPLE_TEAM_ID=...  and optionally ASC_KEY_ID=... ASC_ISSUER_ID=...
#   AuthKey_<ASC_KEY_ID>.p8               App Store Connect API key (optional, enables the upload)
#   otel.env                              OTEL_EXPORTER_OTLP_ENDPOINT=https://...  (optional; without it
#                                         the build has telemetry off and sends nothing)
#
# FLUTTER overrides the flutter executable (CI pins 3.47.9).
# BUILD_NUMBER overrides the build number (default: minutes since the epoch, always increasing).
set -euo pipefail

cd "$(dirname "$0")/.."
keys="${KEYS_DIR:-$HOME/.keys/hardware-spike}"
platform="${1:-}"
revision=$(git rev-parse HEAD)
build_number="${BUILD_NUMBER:-$(( $(date +%s) / 60 ))}"
flutter_bin="${FLUTTER:-flutter}"
common=(--release "--build-number=$build_number" "--dart-define=SOURCE_REVISION=$revision")
if [ -f "$keys/otel.env" ]; then
  # shellcheck disable=SC1091
  . "$keys/otel.env"
fi
if [ -n "${OTEL_EXPORTER_OTLP_ENDPOINT:-}" ]; then
  common+=("--dart-define=OTEL_EXPORTER_OTLP_ENDPOINT=$OTEL_EXPORTER_OTLP_ENDPOINT")
  echo "telemetry: reporting to $OTEL_EXPORTER_OTLP_ENDPOINT" >&2
else
  echo "telemetry: off (no OTEL_EXPORTER_OTLP_ENDPOINT in $keys/otel.env)" >&2
fi

if [ -n "$(git status --porcelain -- ../../crates ../../flutter ../../examples/hardware-spike 2>/dev/null)" ]; then
  echo "warning: uncommitted changes; SOURCE_REVISION ($revision) will not describe this build" >&2
fi

android() {
  [ -f "$keys/key.properties" ] || { echo "no upload key: run tool/setup_android_key.sh" >&2; exit 1; }
  "$flutter_bin" build appbundle "${common[@]}"
  local aab=build/app/outputs/bundle/release/app-release.aab
  # Play rejects a debug-signed bundle; check it is the upload key.
  keytool -printcert -jarfile "$aab" | grep -E "Owner:|SHA256:"
  echo "android: $aab (build $build_number). Upload it in Play Console > Testing > Internal testing."
}

ios() {
  [ -f "$keys/ios.env" ] || { echo "missing $keys/ios.env (APPLE_TEAM_ID=...)" >&2; exit 1; }
  # shellcheck disable=SC1091
  . "$keys/ios.env"
  : "${APPLE_TEAM_ID:?APPLE_TEAM_ID is not set in ios.env}"

  echo "DEVELOPMENT_TEAM = $APPLE_TEAM_ID" > ios/Flutter/Signing.xcconfig
  cat > ios/ExportOptions.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>method</key><string>app-store-connect</string>
	<key>teamID</key><string>$APPLE_TEAM_ID</string>
	<key>signingStyle</key><string>automatic</string>
	<key>uploadSymbols</key><true/>
</dict>
</plist>
EOF

  "$flutter_bin" build ipa "${common[@]}" --export-options-plist=ios/ExportOptions.plist
  ../../flutter/particle_code/tool/check_ios_app.sh build/ios/archive/Runner.xcarchive/Products/Applications/Runner.app

  local ipa
  ipa=$(ls build/ios/ipa/*.ipa | head -n 1)
  if [ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ]; then
    API_PRIVATE_KEYS_DIR="$keys" xcrun altool --upload-app --type ios --file "$ipa" \
      --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
    echo "ios: uploaded $ipa (build $build_number); it shows in TestFlight after processing."
  else
    echo "ios: $ipa (build $build_number). No ASC key configured: drag it into the Transporter app."
  fi
}

case "$platform" in
  android) android ;;
  ios) ios ;;
  both) android; ios ;;
  *) echo "usage: tool/release.sh android|ios|both" >&2; exit 2 ;;
esac
