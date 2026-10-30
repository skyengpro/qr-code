import 'dart:typed_data';

/// Sample payloads shaped like the COSE_Sign1 documents Particle Code carries.
///
/// Both are generated deterministically here (no assets, no randomness) with a tiny CBOR
/// writer, so the same bytes come out on every run. The signature is a pseudo-random
/// stand-in, not a real one.
abstract final class SamplePayloads {
  /// About 200 bytes: an event ticket.
  static final Uint8List ticket = _coseSign1(
    claims: (_) => [
      ('iss', 'https://tickets.example'),
      ('sub', 'attendee-4821'),
      ('evt', 'Particle Night 2026'),
      ('seat', 'B-17'),
      ('iat', 1767225600),
      ('exp', 1767312000),
    ],
  );

  /// About 6 KB: a credential with a large embedded blob (think portrait photo).
  static final Uint8List credential = _fit(6000);

  static Uint8List _fit(int target) {
    // The blob length changes the CBOR header size, so settle on it in two passes.
    var blob = target;
    var out = _credentialWith(blob);
    blob += target - out.length;
    out = _credentialWith(blob);
    return out;
  }

  static Uint8List _credentialWith(int blobLength) => _coseSign1(
    claims: (rng) => [
      ('iss', 'https://id.example.gov'),
      ('sub', 'holder-0042-1337'),
      ('vct', 'org.example.drivers_license'),
      ('given_name', 'Ada'),
      ('family_name', 'Lovelace'),
      ('birth_date', '1815-12-10'),
      ('iat', 1767225600),
      ('exp', 1830297600),
      ('portrait', rng.bytes(blobLength)),
    ],
  );
}

/// Builds `18([protected, {}, payload, signature])`, the COSE_Sign1 layout.
Uint8List _coseSign1({
  required List<(String, Object)> Function(_Rng rng) claims,
}) {
  final rng = _Rng(0x5eed);
  final payload = _Cbor()..map(claims(rng));
  final protectedHeader = _Cbor()
    ..mapHeader(1)
    ..integer(1)
    ..integer(-7); // alg: ES256
  return (_Cbor()
        ..tag(18)
        ..arrayHeader(4)
        ..bytes(protectedHeader.toBytes())
        ..mapHeader(0)
        ..bytes(payload.toBytes())
        ..bytes(rng.bytes(64)))
      .toBytes();
}

/// The few CBOR major types the samples need.
class _Cbor {
  final BytesBuilder _out = BytesBuilder(copy: false);

  Uint8List toBytes() => _out.toBytes();

  void _head(int major, int value) {
    final m = major << 5;
    if (value < 24) {
      _out.addByte(m | value);
    } else if (value < 0x100) {
      _out
        ..addByte(m | 24)
        ..addByte(value);
    } else if (value < 0x10000) {
      _out
        ..addByte(m | 25)
        ..addByte(value >> 8)
        ..addByte(value & 0xFF);
    } else {
      _out.addByte(m | 26);
      for (var shift = 24; shift >= 0; shift -= 8) {
        _out.addByte((value >> shift) & 0xFF);
      }
    }
  }

  void integer(int v) => v >= 0 ? _head(0, v) : _head(1, -1 - v);
  void bytes(List<int> b) {
    _head(2, b.length);
    _out.add(b);
  }

  void text(String s) {
    final utf8 = s.codeUnits; // the samples are ASCII
    _head(3, utf8.length);
    _out.add(utf8);
  }

  void arrayHeader(int n) => _head(4, n);
  void mapHeader(int n) => _head(5, n);
  void tag(int n) => _head(6, n);

  void map(List<(String, Object)> entries) {
    mapHeader(entries.length);
    for (final (key, value) in entries) {
      text(key);
      switch (value) {
        case int v:
          integer(v);
        case String v:
          text(v);
        case Uint8List v:
          bytes(v);
        default:
          throw ArgumentError('unsupported value $value');
      }
    }
  }
}

/// A small deterministic generator (xorshift32) for the stand-in signature and blob.
class _Rng {
  _Rng(this._state);

  int _state;

  int _next() {
    var x = _state;
    x ^= (x << 13) & 0xFFFFFFFF;
    x ^= x >> 17;
    x ^= (x << 5) & 0xFFFFFFFF;
    _state = x & 0xFFFFFFFF;
    return _state;
  }

  Uint8List bytes(int n) =>
      Uint8List.fromList([for (var i = 0; i < n; i++) _next() >> 24]);
}
