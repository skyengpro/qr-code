#!/usr/bin/env bash
# Build ParticleCodeFFI.xcframework: the particlecode-ffi static library for iOS device,
# iOS simulator (arm64 + x86_64) and, with --macos, macOS (arm64 + x86_64), plus the C header
# and a clang module map so Swift can `import ParticleCodeFFI`.
#
# macOS only: it needs Xcode (lipo, xcodebuild, strip, ditto) and the Rust Apple targets.
#
#   scripts/build-xcframework.sh [--macos] [--no-zip] [--debug-info] [--flutter]
#
# Output:
#   target/apple/ParticleCodeFFI.xcframework          the framework
#   target/apple/ParticleCodeFFI.xcframework.zip      for a remote SwiftPM binaryTarget
#   apple/ParticleCodeKit/Frameworks/...xcframework   copy the Swift package and podspec use
#                                                     (SwiftPM requires binary targets inside
#                                                     the package; the copy is git-ignored)
#   flutter/particle_code/ios/particle_code/Frameworks/... with --flutter: copy for the Flutter plugin's
#                                                     podspec (ignored by git; pods cannot reach
#                                                     outside their own directory)
#
# Environment:
#   IPHONEOS_DEPLOYMENT_TARGET   iOS minimum (default 13.0)
#   MACOSX_DEPLOYMENT_TARGET     macOS minimum (default 11.0)
#   CARGO_FLAGS                  extra flags for cargo (default: --locked)
set -euo pipefail

usage() {
    sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

WITH_MACOS=0
WITH_ZIP=1
KEEP_DEBUG=0
WITH_FLUTTER=0
for arg in "$@"; do
    case "$arg" in
        --macos) WITH_MACOS=1 ;;
        --no-zip) WITH_ZIP=0 ;;
        --debug-info) KEEP_DEBUG=1 ;;
        --flutter) WITH_FLUTTER=1 ;;
        -h | --help) usage; exit 0 ;;
        *) echo "error: unknown argument '$arg'" >&2; usage >&2; exit 2 ;;
    esac
done

if [ "$(uname -s)" != "Darwin" ]; then
    echo "error: build-xcframework.sh needs macOS with Xcode (lipo, xcodebuild); this is $(uname -s)." >&2
    echo "       Run it on a Mac, or let .github/workflows/apple.yml build it." >&2
    exit 1
fi

# Repo root, whatever the cwd is.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for tool in cargo rustup lipo xcodebuild strip ditto swift; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: '$tool' not found in PATH (install Xcode command line tools / rustup)." >&2
        exit 1
    fi
done

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-13.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
if [ -z "${CARGO_FLAGS+x}" ]; then
    CARGO_FLAGS="--locked"
fi

# Static libraries need no debug info shipped to apps; the release profile has none for our
# own crates, this drops what the prebuilt standard library carries.
if [ "$KEEP_DEBUG" -eq 0 ]; then
    export CARGO_PROFILE_RELEASE_STRIP=debuginfo
fi

IOS_DEVICE=aarch64-apple-ios
IOS_SIM_ARM=aarch64-apple-ios-sim
IOS_SIM_X86=x86_64-apple-ios
MAC_ARM=aarch64-apple-darwin
MAC_X86=x86_64-apple-darwin

TARGETS="$IOS_DEVICE $IOS_SIM_ARM $IOS_SIM_X86"
if [ "$WITH_MACOS" -eq 1 ]; then
    TARGETS="$TARGETS $MAC_ARM $MAC_X86"
fi

# The toolchain in rust-toolchain.toml is the one `cargo` resolves in this directory, so ask
# that one which targets it has.
INSTALLED="$(rustup target list --installed)"
MISSING=""
for t in $TARGETS; do
    if ! printf '%s\n' "$INSTALLED" | grep -qx "$t"; then
        MISSING="$MISSING $t"
    fi
done
if [ -n "$MISSING" ]; then
    echo "error: missing Rust targets:$MISSING" >&2
    echo "       rustup target add$MISSING" >&2
    exit 1
fi

OUT="$ROOT/target/apple"
STAGE="$OUT/stage"
XCF="$OUT/ParticleCodeFFI.xcframework"
LIB=libparticlecode_ffi.a
rm -rf "$STAGE" "$XCF" "$XCF.zip"
mkdir -p "$STAGE/headers"

# ---- 1. build the static library per target ------------------------------------------------
# `cargo rustc --crate-type staticlib` builds only the .a: the crate also declares a cdylib,
# which an iOS build must not link.
for t in $TARGETS; do
    echo "==> cargo build ($t)"
    # shellcheck disable=SC2086 # CARGO_FLAGS is a word list on purpose
    cargo rustc $CARGO_FLAGS -p particlecode-ffi --lib --release --target "$t" --crate-type staticlib
    if [ ! -f "target/$t/release/$LIB" ]; then
        echo "error: target/$t/release/$LIB was not produced" >&2
        exit 1
    fi
done

# ---- 2. headers + module map ----------------------------------------------------------------
cp crates/particlecode-ffi/include/particlecode.h "$STAGE/headers/particlecode.h"
cat > "$STAGE/headers/module.modulemap" <<'EOF'
module ParticleCodeFFI {
    header "particlecode.h"
    export *
}
EOF

# ---- 3. merge architectures ------------------------------------------------------------------
merge() { # merge <out-dir> <lib>...
    local dir="$1"
    shift
    mkdir -p "$dir"
    lipo -create "$@" -output "$dir/$LIB"
}

merge "$STAGE/ios-sim" "target/$IOS_SIM_ARM/release/$LIB" "target/$IOS_SIM_X86/release/$LIB"
mkdir -p "$STAGE/ios"
cp "target/$IOS_DEVICE/release/$LIB" "$STAGE/ios/$LIB"
if [ "$WITH_MACOS" -eq 1 ]; then
    merge "$STAGE/macos" "target/$MAC_ARM/release/$LIB" "target/$MAC_X86/release/$LIB"
fi

# Remove leftover debug symbols from the archives. Failure is not fatal: the library works
# either way, it is just bigger.
if [ "$KEEP_DEBUG" -eq 0 ]; then
    for lib in "$STAGE"/*/"$LIB"; do
        if strip -S "$lib" -o "$lib.stripped" 2>/dev/null; then
            mv "$lib.stripped" "$lib"
        else
            rm -f "$lib.stripped"
            echo "warning: could not strip $lib; keeping it as built" >&2
        fi
    done
fi

# ---- 4. xcframework -----------------------------------------------------------------------
echo "==> xcodebuild -create-xcframework"
XCARGS=(-library "$STAGE/ios/$LIB" -headers "$STAGE/headers"
    -library "$STAGE/ios-sim/$LIB" -headers "$STAGE/headers")
if [ "$WITH_MACOS" -eq 1 ]; then
    XCARGS+=(-library "$STAGE/macos/$LIB" -headers "$STAGE/headers")
fi
xcodebuild -create-xcframework "${XCARGS[@]}" -output "$XCF"

# The Swift package and the podspec reference this copy (SwiftPM rejects binary targets
# outside the package root).
PKG_COPY="$ROOT/apple/ParticleCodeKit/Frameworks/ParticleCodeFFI.xcframework"
rm -rf "$PKG_COPY"
mkdir -p "$(dirname "$PKG_COPY")"
cp -R "$XCF" "$PKG_COPY"

# The Flutter plugin's podspec vendors its own copy for the same reason.
if [ "$WITH_FLUTTER" -eq 1 ]; then
    FLUTTER_COPY="$ROOT/flutter/particle_code/ios/particle_code/Frameworks/ParticleCodeFFI.xcframework"
    rm -rf "$FLUTTER_COPY"
    mkdir -p "$(dirname "$FLUTTER_COPY")"
    cp -R "$XCF" "$FLUTTER_COPY"
    echo "==> copied to flutter/particle_code/ios/particle_code/Frameworks/"
fi

# ---- 5. zip + checksum ----------------------------------------------------------------------
if [ "$WITH_ZIP" -eq 1 ]; then
    echo "==> zip"
    (cd "$OUT" && ditto -c -k --sequesterRsrc --keepParent ParticleCodeFFI.xcframework ParticleCodeFFI.xcframework.zip)
    CHECKSUM="$(swift package --package-path "$ROOT/apple/ParticleCodeKit" compute-checksum "$XCF.zip")"
fi

# ---- 6. report ------------------------------------------------------------------------------
echo
echo "Static library per target, as built:"
for t in $TARGETS; do
    printf '  %-24s %s\n' "$t" "$(du -h "target/$t/release/$LIB" | cut -f1)"
done
echo "Slices in the xcframework:"
for lib in "$XCF"/*/"$LIB"; do
    printf '  %-34s %s\n' "$(basename "$(dirname "$lib")")" "$(du -h "$lib" | cut -f1)"
done
printf 'xcframework: %s (%s)\n' "$XCF" "$(du -sh "$XCF" | cut -f1)"
if [ "$WITH_ZIP" -eq 1 ]; then
    printf 'zip:         %s.zip (%s)\n' "$XCF" "$(du -h "$XCF.zip" | cut -f1)"
    echo
    echo "SwiftPM checksum (remote binaryTarget):"
    echo "  $CHECKSUM"
fi
echo
echo "Linked app size is far smaller than the archive: the linker keeps only what is used"
echo "(see apple/README.md)."
