# particle_code, iOS side

Swift plugin (iOS 15+) over `apple/ParticleCodeKit`: an `AVCaptureSession` feeds camera frames to
the Rust decoder in place, a platform view shows the preview, and the sender is reached from Dart
through `dart:ffi`. The channel contract is `../PROTOCOL.md`.

## Build steps

On a Mac with Xcode and the Rust Apple targets (`rustup target add aarch64-apple-ios
aarch64-apple-ios-sim x86_64-apple-ios`):

```sh
scripts/build-xcframework.sh --flutter     # from the repository root
```

This builds `target/apple/ParticleCodeFFI.xcframework` (`$CARGO_TARGET_DIR/apple/...` when that is set) and, because of `--flutter`, copies it to
`flutter/particle_code/ios/particle_code/Frameworks/` (git-ignored). The copy is needed because
both a Swift package's binary target and a pod's vendored framework must live inside their own
directory. Without `--flutter` run the script as usual and copy by hand:

```sh
mkdir -p flutter/particle_code/ios/particle_code/Frameworks
cp -R "${CARGO_TARGET_DIR:-target}"/apple/ParticleCodeFFI.xcframework flutter/particle_code/ios/particle_code/Frameworks/
```

Then `flutter build ios` in an app that depends on `particle_code`.

## Swift Package Manager and CocoaPods

The plugin supports both; Flutter picks Swift Package Manager when it is enabled (the default
in current Flutter) and falls back to CocoaPods otherwise
(`flutter config --[no-]enable-swift-package-manager`).

* **SwiftPM:** `particle_code/Package.swift` declares the `particle-code` library: the
  `particle_code` Swift target, the `ParticleCodeFFI` binary target (the xcframework) and the
  `FlutterFramework` dependency Flutter provides. Nothing to add to the app.
* **CocoaPods:** `particle_code.podspec` compiles the same `particle_code/Sources/particle_code`
  files and vendors the same xcframework.

CI builds the example app with SwiftPM (debug and release) and a fresh app created with SwiftPM
disabled with CocoaPods (release), and checks every build for the `dart:ffi` symbols.

The app's `Info.plist` needs a camera explanation, or iOS kills the app when the scanner starts:

```xml
<key>NSCameraUsageDescription</key>
<string>The camera reads Particle Codes shown on another screen.</string>
```

## Source layout

* `particle_code/Sources/particle_code/*.swift` is the plugin (the SwiftPM layout, which the
  podspec reuses). `particle_code/Sources/particle_code/ParticleCodeKit/` holds one committed
  relative symlink per file of `apple/ParticleCodeKit/Sources/ParticleCodeKit`, so the
  `ParticleCodeKit` package and pod and this plugin compile the same files. It is deliberately not a
  symlinked directory: CocoaPods lists pod files with a `**` glob that does not descend into
  symlinked directories. When you add a file to the Kit, add its link here;
  `tool/check-kit-links.sh` (run in CI) fails otherwise.
* `import ParticleCodeFFI` resolves through the xcframework's `module.modulemap`.
* `PrivacyInfo.xcprivacy` is the plugin's (empty) privacy manifest: it tracks nothing and uses no
  required-reason APIs. The app still declares its own camera usage string.

## Why the symbol keeper

Dart calls the `pc_encoder_*` functions with `DynamicLibrary.process()`, so no native code
references them. A static library is linked lazily (`-lparticlecode_ffi` loads only members that
resolve an undefined symbol), and release builds run `-dead_strip`. Left alone, the encoder would
be missing and the Dart lookup would fail at run time, in release builds only.

* **`SymbolKeeper.swift`** references every `pc_*` function from code reachable from
  `register(with:)` (behind a flag the compiler cannot fold, so nothing is called). References
  from live code survive both lazy loading and `-dead_strip`, and need no linker flags. Keep it
  in step with `particlecode.h`; `tool/check-symbols.sh` fails when a function is missing, and
  CI checks that the built app (debug and release) has every `pc_*` as a global symbol.
* **Strip Style "Non-Global Symbols"** must be the host app's setting, so `strip` leaves the
  global `pc_*` symbols that `dlsym` looks for. With CocoaPods the podspec sets it
  (`user_target_xcconfig`). A Swift package cannot change the app's build settings, so with
  SwiftPM set `STRIP_STYLE = non-global` on the Runner target's Release and Profile
  configurations yourself (the example app does).
* `-force_load` of the archive is deliberately not used: CocoaPods does not declare the copied
  xcframework slice as a build input, so Xcode rejects the path ("Build input file cannot be
  found"), and the keeper already makes it unnecessary.
