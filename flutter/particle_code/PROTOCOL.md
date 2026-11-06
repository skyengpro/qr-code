# particle_code plugin: internal contract

This is the contract between the Dart side (`lib/`), the Android side (`android/`) and the iOS side
(`ios/`). Change it only together with every side.

## Architecture

| Role | Where it runs | How it reaches Rust |
| --- | --- | --- |
| **Sender**: encode a payload, paint animated frames | Dart (`CustomPainter`) | flutter_rust_bridge → `crates/particlecode-ffi/src/api` |
| **Scanner camera**: capture, preview, focus, torch | Native (CameraX / AVFoundation) | does not: it hands frames to Dart by address |
| **Scanner decoding**: frame → decoder → progress | Dart orchestration (`lib/src/scanner/`), Rust decoder | flutter_rust_bridge (`ParticleDecoder.feedCameraFrame`) |

Rust is reached **only** through flutter_rust_bridge. The native sides contain no Rust and link
no Rust library. Camera pixels are never copied: native code passes the plane addresses to Dart,
Dart passes them to Rust, Rust reads the planes in place on a worker thread, and Dart tells
native code to release the frame afterwards.

### The Rust library

`hook/build.dart` is a Native Assets build hook: Flutter runs it when building any app that
depends on this plugin. It compiles `crates/particlecode-ffi` with cargo (toolchain and targets
from `crates/particlecode-ffi/rust-toolchain.toml`) for the target being built and bundles the
result: `libparticlecode_ffi.so` in the APK, `particlecode_ffi.framework` in the iOS app.
`ParticleCode.init()` lets flutter_rust_bridge's default loader open it; under `flutter test`
it opens `build/native_assets/<os>/` (`lib/src/native_library.dart`). Building an app
therefore needs `rustup` on the machine; the hook installs the pinned toolchain and targets.

Android additionally builds `libparticle_code_jni.so` from `android/src/main/cpp/` (CMake): a
few lines of C that return the address of a direct `ByteBuffer`, which Kotlin cannot read
itself. It is not Rust and has nothing to do with the decoder.

## Method channel `particle_code/scanner`

| Method | Arguments | Result | Errors (`PlatformException.code`) |
| --- | --- | --- | --- |
| `requestPermission` | none | `bool` granted | none |
| `start` | `{"torch": bool?}` | `null` once the camera runs | `permission_denied`, `no_camera`, `camera_error` |
| `stop` | none | `null` (idempotent) | none |
| `setTorch` | `{"on": bool}` | `bool` whether a torch exists | none |
| `releaseFrame` | `{"id": int}` | `null`; unknown ids are ignored | none |

## Event channel `particle_code/camera_frames`

Listening does not start the camera; `start` does. Each event is a `Map<String, Object?>` with a
`"type"` key.

```text
{"type": "frame",
 "id": int,                             // increasing per process; the handle for releaseFrame
 "width": int, "height": int,           // buffer size in pixels, sensor orientation
 "yAddress": int, "yLen": int,          // plane base addresses (bytes) and readable lengths
 "uAddress": int, "uLen": int,
 "vAddress": int, "vLen": int,
 "yStride": int,                        // bytes per luma row
 "uvStride": int,                       // bytes per chroma row
 "uvPixelStride": int,                  // 2 for interleaved chroma (NV21/NV12), 1 for planar
 "toPreview": [a, b, c, d, e, f]}       // affine map buffer px -> preview, normalized 0..1:
                                        //   x' = a*x + b*y + c,  y' = d*x + e*y + f

{"type": "error", "code": String, "message": String}   // camera failures after start
```

### Frame ownership (the handoff)

1. Native acquires a camera frame and keeps it **locked**: Android keeps the `ImageProxy` open,
   iOS keeps the `CMSampleBuffer` retained and its `CVPixelBuffer` base address locked
   (read-only). It sends a `frame` event with the plane addresses.
2. **At most one frame is in flight.** While a frame is unreleased, native drops new camera
   frames (Android: return early and close them; `STRATEGY_KEEP_ONLY_LATEST` makes the next one
   delivered the newest; iOS: `alwaysDiscardsLateVideoFrames` and skip in the delegate). So
   decoding always works on the newest frame and never queues.
3. Dart calls `ParticleDecoder.feedCameraFrame` with the addresses; Rust reads the planes on a
   worker thread. Rust rejects null addresses, empty planes and lengths that do not cover
   `width`, `height` and the strides, without reading.
4. When that call has **completed** (also when it threw), Dart calls `releaseFrame(id)`. Only
   then does native unlock and release the frame. Dart releases every frame it receives exactly
   once, including frames it decides not to decode (for example after `done` or while stopping).
5. `stop`: Dart first waits for an in-flight decode to complete and releases its frame, then
   calls `stop`. Native `stop` stops capture; if a frame is still unreleased (Dart broke the
   contract), native keeps it until `releaseFrame` or the next `start`.
6. `start` releases any frame still held from a previous session (a hot restart loses the Dart
   side; its Rust call finished long before the next `start`).

`toPreview` maps a point of the frame buffer (the anchors Rust returns are in buffer pixels) to
the preview platform view, normalized to the view's size (0..1, origin top left), including
rotation to the displayed orientation, mirroring and the aspect-fill crop. Points the crop cut
off land outside 0..1. Native computes it from the platform's own transforms (Android:
inverse of `ImageProxy.imageInfo.sensorToBufferTransformMatrix`, then
`PreviewView.sensorToViewTransform`, divided by the view size; iOS: three buffer points through
`layerPointConverted(fromCaptureDevicePoint:)`). Without a laid-out preview it is the upright
image itself: the buffer rotated clockwise by the frame's rotation and divided by its size.

The Android `YUV_420_888` planes map directly (`U` and `V` are usually the same interleaved
buffer one byte apart, `uvPixelStride` 2). iOS uses
`kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (NV12): `uAddress` is the base of plane 1
(interleaved CbCr), `vAddress` that + 1, `uvPixelStride` 2, `uLen` plane 1's
`bytesPerRow * heightOfPlane` and `vLen` one byte less.

## Dart scanner events

Native emits no progress. `ParticleScanner` (Dart) turns `feedCameraFrame` results into its
public `ScanEvent`s (`ScanFrame`, `ScanSegment`, `ScanDone`, `ScanError`), maps the anchors
through `toPreview`, derives the hint, and keeps the decoder; `reset` is a new decoder, the
camera keeps running. `ScanFrame` events are rate-limited to about 10 per second, and always
emitted when progress changes.

## Method channel `particle_code/display`

| Method | Arguments | Result |
| --- | --- | --- |
| `setShowingCode` | `{"on": bool}` | `null`. `on`: remember the current brightness, force maximum, keep the screen on. `off`: restore both. Idempotent. |

## Platform view `particle_code/preview`

A native camera preview (Android `PreviewView`, iOS `AVCaptureVideoPreviewLayer` with
`resizeAspectFill`). It shows the camera of the running scanner session. If it is created before
`start`, it attaches when the session starts. It takes no creation params; Dart passes `null`.
`toPreview` of each frame is normalized to this view's displayed content.

## Sender painter contract

The painter follows `crates/particlecode-ffi/README.md` ("Painter contract"):
* Frames are the typed `FrameDto` of the bridge (`FrameSpec.fromDto`; the JSON form is the same
  data), in pitch units with y down.
* Use the exact sRGB palette (`profile::colors`).
* The data frame changes every 1000/fps ms on a timer, independent of paint.
* 40 ms cross-fade between data frames.
* Breathing wave ≤ 0.12 p drift and ±5 % radius pulse.
* Particles at their encoded sector angles.
* Max brightness while the code shows.
