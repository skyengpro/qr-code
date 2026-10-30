#!/usr/bin/env bash
# Fails if a pc_* function declared in particlecode.h is not referenced by
# particle_code/Sources/particle_code/SymbolKeeper.swift (it would be dead-stripped from the app, and Dart's
# DynamicLibrary.process() lookup would fail in release builds).
#
#   flutter/particle_code/ios/tool/check-symbols.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HEADER="$HERE/../../../../crates/particlecode-ffi/include/particlecode.h"
KEEPER="$HERE/../particle_code/Sources/particle_code/SymbolKeeper.swift"

missing=0
# Declarations look like `type pc_name(` at the start of a line or after `*`.
for fn in $(grep -oE '\bpc_[a-z0-9_]+\(' "$HEADER" | tr -d '(' | sort -u); do
    # A call in the keeper is `pc_name(`; the doc comments do not use the parenthesis form.
    if ! grep -qE "\b${fn}\(" "$KEEPER"; then
        echo "error: $fn is declared in particlecode.h but not referenced in SymbolKeeper.swift" >&2
        missing=1
    fi
done
exit "$missing"
