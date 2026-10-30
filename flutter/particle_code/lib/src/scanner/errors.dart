import 'package:flutter/services.dart';

/// Why a scanner call failed (the `PlatformException.code` of `PROTOCOL.md`).
enum ParticleScannerError {
  /// The camera permission was denied (`permission_denied`).
  permissionDenied('permission_denied'),

  /// The device has no usable camera (`no_camera`).
  noCamera('no_camera'),

  /// The camera failed to open or died (`camera_error`).
  cameraError('camera_error'),

  /// The platform has no scanner implementation (no plugin registered).
  unsupported('unsupported'),

  /// Any other code.
  unknown('unknown');

  const ParticleScannerError(this.code);

  /// The protocol code string.
  final String code;

  /// Maps a protocol code to an enum value; unknown codes become [unknown].
  static ParticleScannerError fromCode(String? code) {
    for (final e in values) {
      if (e.code == code) return e;
    }
    return unknown;
  }
}

/// A scanner method failed.
class ParticleScannerException implements Exception {
  /// Creates an exception.
  const ParticleScannerException(this.error, this.message, {this.code});

  /// Maps a [PlatformException] to a typed exception.
  factory ParticleScannerException.fromPlatform(PlatformException e) =>
      ParticleScannerException(
        ParticleScannerError.fromCode(e.code),
        e.message ?? e.code,
        code: e.code,
      );

  /// What went wrong.
  final ParticleScannerError error;

  /// Description from the platform, for logs.
  final String message;

  /// The raw platform error code, if the error came from the platform.
  final String? code;

  @override
  String toString() => 'ParticleScannerException(${error.name}): $message';
}
