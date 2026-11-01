#!/usr/bin/env bash
# Build the browser module: particlecode-wasm compiled to wasm32-unknown-unknown and run
# through wasm-bindgen, as an ES module that needs no bundler.
#
#   scripts/build-wasm.sh [out-dir]
#
# Output (default out-dir: <target>/wasm-pkg, where <target> is ${CARGO_TARGET_DIR:-target}):
#   particlecode_wasm.js         ES module glue (--target web, no TypeScript declarations)
#   particlecode_wasm_bg.wasm    the code; load it with `initSync({ module: bytes })` or `init()`
#
# Needs: the wasm32-unknown-unknown Rust target (`rustup target add wasm32-unknown-unknown`)
# and the wasm-bindgen CLI at exactly the version this workspace pins
# (`cargo install --locked wasm-bindgen-cli --version <pin>`; the pin is the `wasm-bindgen`
# line in the workspace Cargo.toml and this script prints it when it refuses a CLI).
#
# Environment:
#   CARGO_TARGET_DIR   cargo's build directory (default: target in the repo root)
#   CARGO_FLAGS        extra flags for cargo (default: --locked)
#   WASM_BINDGEN       the wasm-bindgen executable (default: wasm-bindgen on PATH)
set -euo pipefail

usage() {
    sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

case "${1:-}" in
    -h | --help) usage; exit 0 ;;
    -*) echo "error: unknown option '$1'" >&2; usage >&2; exit 2 ;;
esac
if [ "$#" -gt 1 ]; then
    echo "error: expected at most one argument (the output directory)" >&2
    exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

die() {
    echo "error: $*" >&2
    exit 1
}

# The pin lives once, in the workspace manifest; the crate inherits it with `workspace = true`.
PIN="$(sed -n 's/^wasm-bindgen = "=\([0-9][0-9.]*\)".*$/\1/p' Cargo.toml)"
[ -n "$PIN" ] || die "cannot read the wasm-bindgen pin (an exact \`wasm-bindgen = \"=x.y.z\"\`) from $ROOT/Cargo.toml"

WASM_BINDGEN="${WASM_BINDGEN:-wasm-bindgen}"
command -v "$WASM_BINDGEN" > /dev/null 2>&1 \
    || die "wasm-bindgen CLI not found; install it with: cargo install --locked wasm-bindgen-cli --version $PIN"
CLI_VERSION="$("$WASM_BINDGEN" --version | awk '{print $2}')"
[ "$CLI_VERSION" = "$PIN" ] \
    || die "wasm-bindgen CLI is $CLI_VERSION but the workspace pins $PIN (the glue and the module must match); install it with: cargo install --locked wasm-bindgen-cli --version $PIN --force"

command -v cargo > /dev/null 2>&1 || die "cargo not found; install Rust from https://rustup.rs"
if command -v rustup > /dev/null 2>&1; then
    rustup target list --installed | grep -qx wasm32-unknown-unknown \
        || die "missing Rust target; run: rustup target add wasm32-unknown-unknown"
fi

# Where cargo puts its output; made absolute so every path below agrees with cargo.
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
mkdir -p "$TARGET_DIR"
TARGET_DIR="$(cd "$TARGET_DIR" && pwd)"
OUT="${1:-$TARGET_DIR/wasm-pkg}"

if [ -z "${CARGO_FLAGS+x}" ]; then
    CARGO_FLAGS="--locked"
fi

# `release-wasm` (workspace Cargo.toml) is the size-oriented profile; it inherits `release`.
echo "==> cargo build (wasm32-unknown-unknown, release-wasm)"
# shellcheck disable=SC2086 # CARGO_FLAGS is a word list on purpose
CARGO_TARGET_DIR="$TARGET_DIR" cargo build $CARGO_FLAGS --profile release-wasm \
    --target wasm32-unknown-unknown -p particlecode-wasm

WASM="$TARGET_DIR/wasm32-unknown-unknown/release-wasm/particlecode_wasm.wasm"
[ -f "$WASM" ] || die "$WASM was not produced"

echo "==> wasm-bindgen $CLI_VERSION --target web"
mkdir -p "$OUT"
"$WASM_BINDGEN" --target web --no-typescript --out-name particlecode_wasm --out-dir "$OUT" "$WASM"

echo
echo "Built:"
ls -l "$OUT/particlecode_wasm.js" "$OUT/particlecode_wasm_bg.wasm"
