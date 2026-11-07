#!/usr/bin/env bash
# Creates the Android upload keystore once, under ~/.keys/hardware-spike/.
# BACK UP that directory: losing the upload key means a reset request to Google Play.
set -euo pipefail

dir="${KEYS_DIR:-$HOME/.keys/hardware-spike}"
store="$dir/upload-keystore.jks"
props="$dir/key.properties"

if [ -f "$store" ]; then
  echo "keystore already exists: $store" >&2
  exit 0
fi

mkdir -p "$dir"
chmod 700 "$dir"

password=$(openssl rand -base64 24 | tr -d '/+=\n')
keytool -genkeypair -v \
  -keystore "$store" -storetype PKCS12 \
  -alias upload -keyalg RSA -keysize 2048 -validity 10000 \
  -storepass "$password" -keypass "$password" \
  -dname "CN=Particle Hardware Spike, O=vaam, C=CM"

cat > "$props" <<EOF
storeFile=$store
storePassword=$password
keyAlias=upload
keyPassword=$password
EOF
chmod 600 "$store" "$props"

echo "created $store and $props"
