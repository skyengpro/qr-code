# particle_code: Android side

Kotlin implementation of the channels in `../PROTOCOL.md`: `particle_code/scanner`,
`particle_code/camera_frames`, `particle_code/display` and the `particle_code/preview` platform
view. It is only a camera: no decoder, no Rust, no Rust library. CameraX frames go to Dart by
plane address; Dart passes them to Rust through flutter_rust_bridge (the Rust library is built
and bundled by the Native Assets hook `../hook/build.dart`).

## Build

Gradle, AGP and Kotlin versions follow `flutter create --template=plugin` of Flutter 3.47
(AGP 9.1, Kotlin 2.4, JDK 17), `minSdk` 24, `compileSdk` 36, CameraX 1.4.1. `flutter build apk`
in any app that depends on the plugin is all it takes.

The only native code in the plugin is `src/main/cpp/particle_code_jni.c`, built with CMake into
`libparticle_code_jni.so`: one JNI function, `DirectBuffers.address`, that returns the address of
a direct `ByteBuffer` (Kotlin cannot). `consumer-rules.pro` keeps that class un-renamed in
minified apps.

## Frames

`ScannerSession` binds a CameraX `Preview` and an `ImageAnalysis` (`YUV_420_888`,
`STRATEGY_KEEP_ONLY_LATEST`, about 1080p, 16:9). `FrameStore` holds the one frame in flight:

* The analyzer offers each `ImageProxy` to the store. If a frame is unreleased, the new one is
  closed at once. Otherwise it stays open, and a `frame` event with the plane addresses
  (`DirectBuffers.address + buffer.position()`), lengths (`remaining()`), strides and `toPreview`
  is posted to the main thread.
* `releaseFrame(id)` (main thread) closes it. Unknown ids are ignored.
* `stop` does not close an in-flight frame; `start` on a new session does (`releaseAll`). So does
  an event nobody listens to, and engine detach.
* The store is guarded by a lock: the analyzer thread offers, the main thread releases. The main
  thread also checks `isHeld(id)` right before emitting, so Dart never gets an id that a
  concurrent `start` has already closed.

`toPreview` is computed on the main thread (`PreviewView.sensorToViewTransform` requires it):
inverse of `sensorToBufferTransformMatrix`, then the view transform, divided by the view size, as
`[a, b, c, d, e, f]`. Without a laid-out preview it is the upright image (rotated clockwise by
`rotationDegrees`, divided by its size).

## Permission

The plugin's manifest declares `android.permission.CAMERA` and the optional feature
`android.hardware.camera.any` (so devices without a camera can still show codes). The runtime
permission is requested by `requestPermission`. `start` does not prompt: it fails with
`permission_denied` when the permission is missing.

## Preview platform view

`particle_code/preview` creates a CameraX `PreviewView` in `COMPATIBLE` mode (a `TextureView`) with
`FILL_CENTER`. `COMPATIBLE` avoids the `SurfaceView` hole-punching of `PERFORMANCE` mode, which
breaks when the view is clipped, transformed or overlapped by Flutter widgets, and is the mode in
which the view transforms are specified to be accurate. Dart embeds it with `AndroidView`; check
the preview, overlay alignment and tap-to-focus on a real device.

The view can be created before or after `start`: the camera's `Preview` renders into the newest
view that exists. A tap focuses and meters at that point. The camera also refocuses at the center
once at start and every 4 s (native does not know whether a code is locked).

## Lifecycle

* Configuration changes: the camera is unbound and rebound to the new activity.
* `onDetachedFromActivity` and engine detach: the camera is unbound and the analysis thread is
  shut down. Pending permission requests complete with `false`.
