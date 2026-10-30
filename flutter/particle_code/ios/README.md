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

* `Classes/*.swift` is the plugin. `Classes/ParticleCodeKit/` holds one committed relative
  symlink per file of `apple/ParticleCodeKit/Sources/ParticleCodeKit`, so SwiftPM, the
  `ParticleCodeKit` pod and this plugin compile the same files. It is deliberately not a
  symlinked directory: CocoaPods lists pod files with a `**` glob that does not descend into
  symlinked directories. When you add a file to the Kit, add its link here;
  `tool/check-kit-links.sh` (run in CI) fails otherwise.
* `import ParticleCodeFFI` resolves through the xcframework's `module.modulemap`.

## Why the symbol keeper

Dart calls the `pc_encoder_*` functions with `DynamicLibrary.process()`, so no native code
references them. A static library is linked lazily (`-lparticlecode_ffi` loads only members that
resolve an undefined symbol), and release builds run `-dead_strip`. Left alone, the encoder would
be missing and the Dart lookup would fail at run time, in release builds only.

* **`Classes/SymbolKeeper.swift`** references every `pc_*` function from code reachable from
  `register(with:)` (behind a flag the compiler cannot fold, so nothing is called). References
  from live code survive both lazy loading and `-dead_strip`, and need no linker flags. Keep it
  in step with `particlecode.h`; `tool/check-symbols.sh` fails when a function is missing, and
  CI checks that the built app (debug and release) has every `pc_*` as a global symbol.
* **Strip Style "Non-Global Symbols"** is set for the host app by the podspec
  (`user_target_xcconfig`), so `strip` leaves the global `pc_*` symbols that `dlsym` looks for.
* `-force_load` of the archive is deliberately not used: CocoaPods does not declare the copied
  xcframework slice as a build input, so Xcode rejects the path ("Build input file cannot be
  found"), and the keeper already makes it unnecessary.

## Later: Swift Package Manager

Flutter's SwiftPM support for plugins could replace the podspec: add `ios/particle_code/Package.swift`
with a local `ParticleCodeFFI` binary target and the `ParticleCodeKit` sources, as in
`apple/ParticleCodeKit/Package.swift`. The symbol keeper stays necessary there, since SwiftPM links
static binary targets lazily too.
