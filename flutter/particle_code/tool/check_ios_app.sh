#!/usr/bin/env bash
# Checks that a built iOS app embeds the Rust library the Native Assets hook built, and that
# the library exports the flutter_rust_bridge entry points Dart looks up at run time.
#
#   tool/check_ios_app.sh path/to/Runner.app
set -euo pipefail

app=${1:?usage: check_ios_app.sh path/to/Runner.app}
bin="$app/Frameworks/particlecode_ffi.framework/particlecode_ffi"
if [ ! -f "$bin" ]; then
  echo "missing $bin; frameworks in the app:" >&2
  ls "$app/Frameworks" >&2 || true
  exit 1
fi
# `nm` may exit non-zero on some inputs; judge by its output. These are what Dart looks up:
# the SSE dispatchers every call goes through, the content hash checked at init, and the
# opaque-handle reference counting of the decoder.
exported=$(nm -gU "$bin" 2>/dev/null || true)
for sym in frb_pde_ffi_dispatcher_primary frb_pde_ffi_dispatcher_sync frb_get_rust_content_hash \
           frbgen_particle_code_rust_arc_decrement_strong_count_RustOpaque_flutter_rust_bridgefor_generatedRustAutoOpaqueInnerParticleDecoder; do
  grep -q "_$sym\$" <<< "$exported" || { echo "$sym is not exported by $bin" >&2; exit 1; }
done
echo "$bin exports the flutter_rust_bridge entry points"
