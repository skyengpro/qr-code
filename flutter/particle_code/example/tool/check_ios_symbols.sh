#!/usr/bin/env bash
# Checks that a built iOS app exports every pc_* function of particlecode.h.
#
# Dart reaches the encoder with DynamicLibrary.process(), so the functions must be in the
# app's own Mach-O image, global and not stripped, even though no native code calls them.
# That is what the plugin's SymbolKeeper.swift is for.
#
#   tool/check_ios_symbols.sh build/ios/iphoneos/Runner.app
#
# macOS only (uses nm).
set -euo pipefail

app="${1:?usage: check_ios_symbols.sh path/to/Runner.app}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
header="$here/../../../../crates/particlecode-ffi/include/particlecode.h"

fns=$(grep -oE '\bpc_[a-z0-9_]+\(' "$header" | tr -d '(' | sort -u)
test -n "$fns" || { echo "no pc_* functions found in $header"; exit 1; }

# Debug builds may move the app's code from Runner into Runner.debug.dylib, so look at the
# main executable and every dylib next to it.
images=("$app/Runner")
for f in "$app"/*.dylib; do
    [ -f "$f" ] && images+=("$f")
done

syms=""
for image in "${images[@]}"; do
    test -f "$image" || { echo "missing $image"; exit 1; }
    echo "== $image"
    file "$image"
    # Apple nm can exit non-zero for odd members; judge by the output.
    syms+=$(nm -g "$image" 2>/dev/null || true)$'\n'
done

missing=0
for fn in $fns; do
    # Here-string, not a pipe: with pipefail, grep -q closing early would fail the pipe.
    if grep -qE " T _${fn}\$" <<< "$syms"; then
        echo "ok      $fn"
    else
        echo "MISSING $fn"
        missing=1
    fi
done
if [ "$missing" -ne 0 ]; then
    echo "error: some pc_* symbols are not in the app; DynamicLibrary.process() would fail" >&2
    exit 1
fi
echo "all $(wc -w <<< "$fns" | tr -d ' ') pc_* functions are in the app"
