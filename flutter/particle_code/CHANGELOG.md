# Changelog

## Unreleased

* The demo app moved from `example/` to `examples/flutter-demo/` at the repository root.
* The sender now uses flutter_rust_bridge 2.13 (generated bindings in `lib/src/rust/`)
  instead of hand-written `dart:ffi`, JSON and a worker isolate.
* **Breaking:** call `await ParticleCode.init()` once (for example in `main()`) before
  creating a `ParticleEncoder`, calling `livenessVerify` or showing a `ParticleCodeView`;
  otherwise they throw a `StateError`. `init` is idempotent.
* **Breaking:** removed `ParticleEncoder.createWith`, `ParticleEncoder.fromWorkerSpec`,
  `ParticleEncoder.workerSpec`, `EncoderWorkerSpec` and `debugFrameCalls`; removed
  `FrameSpec.toMessage` / `fromMessage` and `FrameDots.toMessage` / `fromMessage`.
* Added `ParticleEncoder.frameAsync`, `FrameSpec.fromDto` and `ParticleCode.init`.
  `ParticleCodeView` prefetches with `frameAsync`; `useBackgroundIsolate` is deprecated and
  has no effect.
* Frames are built from typed arrays directly: no JSON parsing for painting. Profile M/L
  `frame()` costs about 0.3 / 0.4 ms instead of 3.3 / 5.3 ms. The streaming JSON parser is
  gone; `frameJson` and `FrameSpec.fromJson` remain.
* The `pc_abi_version` check is replaced by the bridge's content hash, verified by `init`.
* Dropped the `ffi` package dependency (it is a dev dependency again, for the scanner tests).
* The scanner now decodes in Dart: the platform only captures camera frames and hands them
  over by address (`particle_code/camera_frames`, `releaseFrame`); `ParticleScanner` feeds
  them to the Rust decoder through flutter_rust_bridge (planes read in place, no copy),
  releases each frame, and builds `ScanFrame` / `ScanSegment` / `ScanDone` / `ScanError`
  itself (anchors mapped through the frame's `toPreview`, hint code from the
  decoder's `FrameResultDto.hintCode`). The `ScanEvent` types, hint codes, `ParticleScannerPreview` and
  the progress ring are unchanged.
* **Breaking:** `ParticleScanner.start` needs `await ParticleCode.init()` and throws a
  `StateError` otherwise. `ParticleScanner.eventsChannel` is now `framesChannel`
  (`particle_code/camera_frames`; the constructor parameter `eventChannel` became
  `framesChannel`). `reset()` no longer calls the platform: it swaps in a new decoder.
  `stop()` waits for the frame in flight before stopping the camera.

## 0.2.0

* Swift Package Manager support on iOS, beside CocoaPods: the plugin builds through
  `ios/particle_code/Package.swift` when the app uses SwiftPM (#4).
* Licensed under MIT (#6).
* `ParticleCodeView` builds frames on a background isolate (`useBackgroundIsolate`, on by
  default): prefetch two frames ahead, liveness second predicted from the data clock,
  synchronous fallback, disposed with the view.
* Frame dots are stored as typed arrays (`FrameDots`, still a `List<FrameDot>`); frames
  cross isolates as typed data (`FrameSpec.toMessage` / `fromMessage`).
* Streaming parser for the frame JSON (about 2.7x faster than `jsonDecode` for profile L)
  with `jsonDecode` fallback.

## 0.1.0

* `ParticleEncoder` over the Rust C ABI (`dart:ffi`, hand-written bindings), `EncoderOptions`,
  `Shape`, `FrameSpec` and `ParticleCodePalette`.
* Dart port of `particlecode_core::motion`, parity-tested against Rust values.
* `ParticleCodePainter` and `ParticleCodeView` (ticker-driven, wall-clock liveness, max
  brightness while showing).
* Scanner API: `ParticleScanner`, typed `ScanEvent`s, `ParticleScannerPreview`,
  `ScanProgressRing`.
