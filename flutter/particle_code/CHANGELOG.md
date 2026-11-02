# Changelog

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
