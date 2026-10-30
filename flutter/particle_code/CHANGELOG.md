# Changelog

## 0.1.0

* `ParticleEncoder` over the Rust C ABI (`dart:ffi`, hand-written bindings), `EncoderOptions`,
  `Shape`, `FrameSpec` and `ParticleCodePalette`.
* Dart port of `particlecode_core::motion`, parity-tested against Rust values.
* `ParticleCodePainter` and `ParticleCodeView` (ticker-driven, wall-clock liveness, max
  brightness while showing).
* Scanner API: `ParticleScanner`, typed `ScanEvent`s, `ParticleScannerPreview`,
  `ScanProgressRing`.
