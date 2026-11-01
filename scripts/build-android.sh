#!/usr/bin/env bash
# Build libparticlecode_jni.so for the Android ABIs into the Gradle module's jniLibs.
#
#   scripts/build-android.sh            release build (what the AAR ships)
#   scripts/build-android.sh --debug    debug build, faster to compile
#
# Honours CARGO_TARGET_DIR (cargo-ndk passes it to cargo; nothing here reads target/).
# Needs: Rust (rustup), cargo-ndk (`cargo install --locked cargo-ndk`) and an Android NDK.
# The NDK is taken from ANDROID_NDK_HOME, ANDROID_NDK_ROOT, ANDROID_NDK_LATEST_HOME (set on
# GitHub's ubuntu runners) or the newest $ANDROID_HOME/ndk/<version>.
set -euo pipefail

ABIS=(arm64-v8a armeabi-v7a x86_64)
RUST_TARGETS=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android)
MIN_API=24 # keep in step with minSdk in android/particlecode/build.gradle.kts

die() {
    echo "error: $*" >&2
    exit 1
}

profile_flag=(--release)
case "${1:-}" in
    "") ;;
    --debug) profile_flag=() ;;
    -h | --help)
        sed -n '2,11p' "$0"
        exit 0
        ;;
    *) die "unknown argument '$1' (try --debug or --help)" ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$repo_root/android/particlecode/src/main/jniLibs"

command -v cargo > /dev/null 2>&1 \
    || die "cargo not found; install Rust from https://rustup.rs"

if ! cargo ndk --version > /dev/null 2>&1; then
    die "cargo-ndk not found; install it with: cargo install --locked cargo-ndk"
fi

# Find the NDK.
ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-${ANDROID_NDK_LATEST_HOME:-}}}"
if [[ -z "$ndk" ]]; then
    sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
    if [[ -n "$sdk" && -d "$sdk/ndk" ]]; then
        # Newest installed version (version-sorted).
        ndk="$(find "$sdk/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1)"
    fi
fi
[[ -n "$ndk" ]] \
    || die "no Android NDK found. Set ANDROID_NDK_HOME, or install one with:
    sdkmanager 'ndk;27.2.12479018'   (and set ANDROID_HOME)
  or via Android Studio: SDK Manager > SDK Tools > NDK (Side by side)."
[[ -d "$ndk" ]] || die "ANDROID_NDK_HOME points to '$ndk', which is not a directory"
[[ -d "$ndk/toolchains/llvm/prebuilt" ]] \
    || die "'$ndk' does not look like an NDK (no toolchains/llvm/prebuilt)"
export ANDROID_NDK_HOME="$ndk"
echo "Using NDK: $ndk"

# Rust standard libraries for the Android targets (no-op if already installed).
if command -v rustup > /dev/null 2>&1; then
    rustup target add "${RUST_TARGETS[@]}"
else
    echo "warning: rustup not found; assuming the Android targets are installed" >&2
fi

# Google Play requires 16 KB page alignment on Android 15+ (NDK r28 does it by default).
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-z,max-page-size=16384"
# Ship stripped libraries; the AAR would otherwise carry tens of MB of debug info.
export CARGO_PROFILE_RELEASE_STRIP=symbols

ndk_args=()
for abi in "${ABIS[@]}"; do
    ndk_args+=(-t "$abi")
done

cd "$repo_root"
rm -rf "$out_dir"
cargo ndk "${ndk_args[@]}" --platform "$MIN_API" -o "$out_dir" \
    build --locked -p particlecode-jni ${profile_flag[@]+"${profile_flag[@]}"}

echo
echo "Built:"
find "$out_dir" -name '*.so' -print0 | sort -z | xargs -0 ls -l
