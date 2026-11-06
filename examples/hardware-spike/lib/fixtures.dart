import 'dart:typed_data';

import 'package:crypto/crypto.dart';

/// A synthetic, deterministic payload. The bytes are pseudo-random, contain no real
/// credential or personal data, and are identical on every device and run, so the
/// receiver can verify a scan byte for byte without any shared secret.
class Fixture {
  const Fixture._(this.id, this.byteCount, this.seed, this.description);

  /// Short identifier recorded in reports.
  final String id;

  /// Payload size in bytes.
  final int byteCount;

  final int seed;

  /// What the size stands for.
  final String description;

  /// The payload bytes (generated on demand; xorshift32 from [seed]).
  Uint8List get bytes {
    final out = Uint8List(byteCount);
    var x = seed;
    for (var i = 0; i < byteCount; i++) {
      x ^= (x << 13) & 0xFFFFFFFF;
      x ^= x >> 17;
      x ^= (x << 5) & 0xFFFFFFFF;
      out[i] = x & 0xFF;
    }
    return out;
  }

  /// Lowercase hex SHA-256 of [bytes].
  String get sha256Hex => sha256.convert(bytes).toString();

  @override
  String toString() => '$id ($byteCount bytes)';
}

/// The fixture set of the spike.
abstract final class Fixtures {
  static const f200 = Fixture._(
    'f200',
    200,
    0x2545F491,
    'ticket-sized (~200 B)',
  );
  static const f1k = Fixture._(
    'f1k',
    1024,
    0x1B873593,
    'small credential (1 KiB)',
  );
  static const f6k = Fixture._('f6k', 6144, 0x7F4A7C15, 'credential (6 KiB)');
  static const f30k = Fixture._(
    'f30k',
    30720,
    0x3C6EF372,
    'large credential (30 KiB)',
  );

  /// All fixtures, smallest first.
  static const List<Fixture> all = [f200, f1k, f6k, f30k];

  /// The fixture with this id, or `null`.
  static Fixture? byId(String id) {
    for (final f in all) {
      if (f.id == id) return f;
    }
    return null;
  }
}

/// Lowercase hex SHA-256 of [data].
String sha256Of(List<int> data) => sha256.convert(data).toString();
