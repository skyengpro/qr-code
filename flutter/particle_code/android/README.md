# particle_code: Android side

Kotlin implementation of the channels in `../PROTOCOL.md`: `particle_code/scanner`,
`particle_code/display`, `particle_code/scan_events` and the `particle_code/preview` platform
view. CameraX feeds frames to the Rust decoder on a background thread; only small progress maps
and the final payload reach Dart.

## Build

The plugin does not carry its own copy of the scanner library or the native code. `build.gradle.kts`
compiles `android/particlecode/src/main/kotlin` and packages `android/particlecode/src/main/jniLibs`
straight from the repository root (`../../..` from this folder), so it only builds inside this
monorepo checkout.

1. Build the native libraries once, from the repository root (needs the Android NDK):

   ```sh
   cargo install --locked cargo-ndk
   scripts/build-android.sh          # --debug for a faster build
   ```

   This writes `libparticlecode_jni.so` for `arm64-v8a`, `armeabi-v7a` and `x86_64` into
   `android/particlecode/src/main/jniLibs`. The file also exports the `pc_*` encoder functions, so
   Dart opens the same library with `DynamicLibrary.open('libparticlecode_jni.so')`.
2. Build the app: `flutter build apk` in the example app (or any app that depends on the plugin).

If `jniLibs` has no `libparticlecode_jni.so`, the Gradle task `checkParticleCodeNativeLibs` (a
dependency of `preBuild`) fails the build with these instructions instead of letting the app crash
with `UnsatisfiedLinkError` at the first scan. A missing ABI only logs a warning.

Toolchain: Gradle, AGP and Kotlin versions follow `flutter create --template=plugin` of Flutter
3.47 (AGP 9.1, Kotlin 2.4, JDK 17), `minSdk` 24. CameraX is 1.4.1, the version the library uses.

## Permission

The plugin's manifest declares `android.permission.CAMERA` and the optional feature
`android.hardware.camera.any` (so devices without a camera can still show codes). The runtime
permission is requested by the `requestPermission` method. `start` does not prompt: it fails with
`permission_denied` when the permission is missing.

## Preview platform view

`particle_code/preview` creates a CameraX `PreviewView` in `COMPATIBLE` mode, which renders through
a `TextureView`. On the Dart side `lib/src/scanner/preview.dart` embeds it with Flutter's
`AndroidView`, not with a `PlatformViewLink` / `initSurfaceAndroidView` hybrid-composition setup.
`AndroidView` leaves the choice of embedding mode to Flutter and the device, so which mode is in
effect has not been checked here.

**Unverified on a device:** nothing in this repository's CI renders the preview, so the properties
below are what the `COMPATIBLE` mode is designed to give, not something measured with `AndroidView`:

* Flutter widgets (the dot overlay, buttons) can be drawn on top of the preview.
* `COMPATIBLE` avoids the `SurfaceView` hole-punching of the default `PERFORMANCE` mode, which
  breaks when the view is clipped, transformed or overlapped. It is also the mode in which the
  anchor coordinate mapping is meant to be exact.
* The cost is a little more GPU work and latency than a `SurfaceView`. The preview is only for the
  user's eyes; decoding does not use it.

Check the preview, overlay alignment and tap-to-focus on a real device before relying on any of it;
if `AndroidView` turns out not to composite correctly, switch `preview.dart` to
`PlatformViewLink` with `initSurfaceAndroidView` and update this section.

The view can be created before or after `start`: the camera's `Preview` renders into the newest
view that exists, and moves to another one when the first is disposed. A tap on the preview
focuses and meters at that point; the plugin also refocuses at the center every 2 s while no code
is locked.

Anchors in `frame` events are normalized to this view (0..1 across its width and height). They
come from the exact sensor-to-view transform of the `PreviewView` (display rotation and the
FILL_CENTER crop included), so points the crop cut off can fall outside 0..1. Without a laid-out
view they fall back to the upright analysis image.

## Lifecycle

* Configuration changes: the camera is unbound and rebound to the new activity; the decoder and
  its progress survive.
* `onDetachedFromActivity` and engine detach: the camera is unbound, the decoder is closed and the
  analysis thread is shut down. Pending permission requests complete with `false`.
* `done` is delivered once. If Dart is not listening at that moment, it is kept and sent when it
  subscribes. After `done` no more frames are decoded until `reset`.
