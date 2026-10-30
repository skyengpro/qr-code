# particle_code plugin: internal contract

This is the contract between the Dart side (`lib/`), the Android side (`android/`) and the iOS side
(`ios/`). Change it only together with every side.

## Architecture

| Role | Where it runs | How it reaches Rust |
| --- | --- | --- |
| **Sender**: encode a payload, paint animated frames | Dart (`CustomPainter`) | `dart:ffi` → C ABI `pc_encoder_*` (`crates/particlecode-ffi/include/particlecode.h`) |
| **Scanner**: camera → decoder → progress | Native (CameraX / AVFoundation) | Android: `particlecode-jni` (`android/particlecode`), iOS: `ParticleCodeKit` (`apple/`) |

Camera frames never cross into Dart. Only small progress events cross, plus the payload at the end.

### Native libraries
* **Android:** a single `libparticlecode_jni.so` per ABI, built by `scripts/build-android.sh` into
  `android/particlecode/src/main/jniLibs`. It exports the JNI functions (scanner) **and** all
  `pc_*` C functions (sender), so Dart opens it with `DynamicLibrary.open('libparticlecode_jni.so')`.
* **iOS:** `ParticleCodeFFI.xcframework` (static, `scripts/build-xcframework.sh`) linked into the
  app. Dart uses `DynamicLibrary.process()`. The plugin must keep every `pc_*` symbol (its Swift
  `SymbolKeeper` references them all, and the podspec sets Strip Style to non-global), because
  the encoder functions are only referenced from Dart and the linker would drop them.
* **Host (tests, desktop):** `DynamicLibrary.open` of `$PARTICLECODE_LIB` if set, else the
  workspace `target/{debug,release}/libparticlecode_ffi.{so,dylib}`.

## Method channel `particle_code/scanner`

| Method | Arguments | Result | Errors (`PlatformException.code`) |
| --- | --- | --- | --- |
| `requestPermission` | none | `bool` granted | none |
| `start` | `{"torch": bool?}` | `null` once the camera runs | `permission_denied`, `no_camera`, `camera_error` |
| `stop` | none | `null` (idempotent) | none |
| `reset` | none | `null`: drop progress, new decoder, keep the camera running | none |
| `setTorch` | `{"on": bool}` | `bool` whether a torch exists | none |

## Method channel `particle_code/display`

| Method | Arguments | Result |
| --- | --- | --- |
| `setShowingCode` | `{"on": bool}` | `null`. `on`: remember the current brightness, force maximum, keep the screen on. `off`: restore both. Idempotent. |

## Event channel `particle_code/scan_events`

Listening does not start the camera; `start` does. Each event is a `Map<String, Object?>` with a
`"type"` key.

```text
{"type": "frame",                       // at most ~10 per second, and always when progress changes
 "status": "rejected" | "accepted" | "done",
 "locked": bool,
 "profile": int,                        // 0 S, 1 M, 2 L, -1 unknown
 "shapeId": int,                        // -1 unknown
 "have": int, "need": int,              // fountain symbols held / needed (sticky across rejects)
 "segmentsDone": int, "segmentsTotal": int,
 "liveness": null | {"consistent": bool, "seedLow": int, "rolling": int},
 "anchors": [x0, y0, x1, y1, ...],      // 0, 3 or 4 points; normalized 0..1 in *preview*
                                        // coordinates (already rotated to the displayed orientation)
 "sharpness": double,
 "hint": String?,                       // user-facing advice, e.g. "Too dark or too much glare"
 "hintCode": int,                       // 0 none, 1 point at code, 2 move back, 3 hold steady,
                                        // 4 lighting, 5 update app, 6 other
 "rejectReason": String?}

{"type": "segment", "bytes": Uint8List}  // newly verified in-order payload bytes (streaming)

{"type": "done", "payload": Uint8List, "sessionId": int}   // once; the scanner stops decoding
                                                            // (camera keeps running) until reset

{"type": "error", "code": String, "message": String}        // camera failures after start
```

## Platform view `particle_code/preview`

A native camera preview (Android `PreviewView`, iOS `AVCaptureVideoPreviewLayer` with
`resizeAspectFill`). It shows the camera of the running scanner session. If it is created before
`start`, it attaches when the session starts. It takes no creation params; Dart passes `null`.
Overlay anchor coordinates are normalized to this view's displayed content.

## Sender painter contract

The painter follows `crates/particlecode-ffi/README.md` ("Painter contract"):
* Frame JSON is the serde `FrameSpec` from `pc_encoder_frame_json`, in pitch units with y down.
* Use the exact sRGB palette (`profile::colors`).
* The data frame changes every 1000/fps ms on a timer, independent of paint.
* 40 ms cross-fade between data frames.
* Breathing wave ≤ 0.12 p drift and ±5 % radius pulse.
* Particles at their encoded sector angles.
* Max brightness while the code shows.
