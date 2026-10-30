# particle_code, iOS side

Swift plugin (iOS 13+) over `apple/ParticleCodeKit`: an `AVCaptureSession` feeds camera frames to
the Rust decoder in place, a platform view shows the preview, and the sender is reached from Dart
through `dart:ffi`. The channel contract is `../PROTOCOL.md`.

## Build steps

On a Mac with Xcode and the Rust Apple targets (`rustup target add aarch64-apple-ios
aarch64-apple-ios-sim x86_64-apple-ios`):

```sh
scripts/build-xcframework.sh --flutter     # from the repository root
```

This builds `target/apple/ParticleCodeFFI.xcframework` and, because of `--flutter`, copies it to
`flutter/particle_code/ios/Frameworks/` (git-ignored). The copy is needed because a pod can only
vendor files inside its own directory. Without `--flutter` run the script as usual and copy by
hand:

```sh
cp -R target/apple/ParticleCodeFFI.xcframework flutter/particle_code/ios/Frameworks/
```

Then `flutter build ios` (or `pod install`) in an app that depends on `particle_code`.

The app's `Info.plist` needs a camera explanation, or iOS kills the app when the scanner starts:

```xml
<key>NSCameraUsageDescription</key>
<string>The camera reads Particle Codes shown on another screen.</string>
```

## Source layout

* `Classes/*.swift` is the plugin. `Classes/ParticleCodeKit` is a committed relative symlink to
  `apple/ParticleCodeKit/Sources/ParticleCodeKit`, so SwiftPM, the `ParticleCodeKit` pod and this
  plugin compile the same files. The podspec names the link explicitly because Ruby's `**` glob
  does not descend into symlinked directories.
* `import ParticleCodeFFI` resolves through the xcframework's `module.modulemap`.

## Why `-force_load` and the symbol keeper

Dart calls the `pc_encoder_*` functions with `DynamicLibrary.process()`, so no native code
references them. A static library is linked lazily (`-lparticlecode_ffi` loads only members that
resolve an undefined symbol), and release builds run `-dead_strip`. Left alone, the encoder would
be missing and the Dart lookup would fail at run time, in release builds only.

Two independent guards, so one failing does not break the app:

1. **`-force_load`** (`user_target_xcconfig` in the podspec) loads the whole archive from
   `${PODS_XCFRAMEWORKS_BUILD_DIR}/particle_code/libparticlecode_ffi.a`. CocoaPods 1.11+ copies
   the matching slice of a vendored static xcframework there (`<pod name>/` holds the `.a` and
   `Headers/`), per SDK, and Runner sees the variable through the Pods xcconfig. `static_framework`
   keeps the plugin inside Runner's link even with `use_frameworks!`. If a future CocoaPods
   changes that directory, the link fails with "file not found": delete the line and rely on 2.
2. **`Classes/SymbolKeeper.swift`** references every `pc_*` function from code reachable from
   `register(with:)` (behind a flag the compiler cannot fold, so nothing is called). References
   from live code survive both lazy loading and `-dead_strip`, and need no linker flags. Keep it
   in step with `particlecode.h`; `tool/check-symbols.sh` fails when a function is missing.

Also keep **Strip Style** at "Non-Global Symbols" (Xcode's default for apps) so `strip` leaves the
global `pc_*` symbols that `dlsym` looks for.

## Later: Swift Package Manager

Flutter's SwiftPM support for plugins could replace the podspec: add `ios/particle_code/Package.swift`
with a local `ParticleCodeFFI` binary target and the `ParticleCodeKit` sources, as in
`apple/ParticleCodeKit/Package.swift`. The symbol keeper stays necessary there, since SwiftPM links
static binary targets lazily too, and `-force_load` has no SwiftPM counterpart.
