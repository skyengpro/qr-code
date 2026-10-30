/// Error raised by the sender side (encoder, FFI library loading).
class ParticleCodeException implements Exception {
  /// Creates an exception carrying [message].
  const ParticleCodeException(this.message);

  /// Human readable description. For encoder errors this is the text produced by the
  /// Rust library.
  final String message;

  @override
  String toString() => 'ParticleCodeException: $message';
}
