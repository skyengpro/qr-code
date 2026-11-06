import 'dart:typed_data';

import '../rust/api/decoder.dart';
import '../rust/api/dto.dart';

/// The decoder behind `ParticleScanner`: one transfer's receiver.
///
/// The real implementation wraps the Rust `ParticleDecoder` (flutter_rust_bridge). The
/// interface exists so tests can script results without camera memory or the Rust
/// library.
abstract interface class FrameDecoder {
  /// Decodes one camera frame in place; completes when Rust no longer reads the planes.
  Future<FrameResultDto> feed(CameraFrameDto frame);

  /// Payload bytes delivered in order since the last call.
  Uint8List? takeReady();

  /// Session id of the transfer, once a frame was accepted.
  int? sessionId();

  /// Releases the native decoder.
  void dispose();
}

/// [FrameDecoder] over the Rust `ParticleDecoder`.
final class RustFrameDecoder implements FrameDecoder {
  /// Creates a fresh decoder. `ParticleCode.init` must have completed.
  RustFrameDecoder() : _decoder = ParticleDecoder();

  final ParticleDecoder _decoder;

  @override
  Future<FrameResultDto> feed(CameraFrameDto frame) =>
      _decoder.feedCameraFrame(frame: frame);

  @override
  Uint8List? takeReady() => _decoder.takeReady();

  @override
  int? sessionId() => _decoder.sessionId();

  @override
  void dispose() => _decoder.dispose();
}
