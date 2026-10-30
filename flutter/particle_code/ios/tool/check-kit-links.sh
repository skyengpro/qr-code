#!/usr/bin/env bash
# Fails unless every Swift file of apple/ParticleCodeKit/Sources/ParticleCodeKit has a
# resolving symlink in
# flutter/particle_code/ios/particle_code/Sources/particle_code/ParticleCodeKit (and nothing else is
# there). The plugin (SwiftPM target and pod) compiles the Kit through those per-file links.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
kit="$here/../../../apple/ParticleCodeKit/Sources/ParticleCodeKit"
links="$here/particle_code/Sources/particle_code/ParticleCodeKit"
status=0
for src in "$kit"/*.swift; do
  name="$(basename "$src")"
  link="$links/$name"
  if [[ ! -L "$link" ]]; then
    echo "missing link: Sources/particle_code/ParticleCodeKit/$name"
    echo "  fix: (cd \"$links\" && ln -s ../../../../../../../apple/ParticleCodeKit/Sources/ParticleCodeKit/$name $name)"
    status=1
  elif ! cmp -s "$link" "$src"; then
    echo "link does not resolve to the Kit source: Sources/particle_code/ParticleCodeKit/$name"
    status=1
  fi
done
for link in "$links"/*; do
  [[ -e "$kit/$(basename "$link")" ]] || { echo "stale entry: Sources/particle_code/ParticleCodeKit/$(basename "$link")"; status=1; }
done
[[ $status -eq 0 ]] && echo "ParticleCodeKit links ok ($(ls "$kit"/*.swift | wc -l) files)"
exit $status
