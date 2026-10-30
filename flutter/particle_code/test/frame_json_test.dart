// The strict fast path for frame JSON must give exactly what jsonDecode gives.
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code/src/frame_json.dart';

Uint8List _payload(int n) =>
    Uint8List.fromList([for (var i = 0; i < n; i++) (i * 17 + 5) & 0xFF]);

FrameSpec _viaJsonDecode(String text) =>
    FrameSpec.fromJson(jsonDecode(text) as Map<String, Object?>);

/// `"dots":[...]` text for the given coordinates, parsed by the dots parser.
FrameDots _parseCoords(List<String> xs) {
  final dots = [
    for (var i = 0; i < xs.length; i++)
      '{"q":${i - 3},"r":${-i},"at":{"x":${xs[i]},"y":${xs[xs.length - 1 - i]}},'
          '"color":${i & 3},"size":${i & 1}}',
  ].join(',');
  final bytes = Uint8List.fromList(utf8.encode('[$dots]'));
  return parseDotsArray(bytes, 1);
}

void main() {
  group('real frames', () {
    for (final (profile, shape) in [
      (CodeProfile.s, Shape.disc),
      (CodeProfile.m, Shape.heart),
      (CodeProfile.l, Shape.letter('K')),
      (CodeProfile.m, Shape.ring),
    ]) {
      test('${profile.letter} ${shape.id}: identical to jsonDecode', () {
        final enc = ParticleEncoder.create(
          _payload(3000),
          options: EncoderOptions(
            profile: profile,
            shape: shape,
            segmentKbLog2: 0,
          ),
        );
        addTearDown(enc.dispose);
        final before = debugFrameJsonFallbacks;
        for (final i in [0, 1, 2, 5, enc.framesPerPass + 1]) {
          final text = enc.frameJson(i, unixSeconds: 1700000000 + i * 37);
          final expected = _viaJsonDecode(text);
          final fast = parseFrameJsonBytes(
            Uint8List.fromList(utf8.encode(text)),
            dotCountHint: enc.info.dotCount,
          );
          expect(fast.sameAs(expected), isTrue, reason: 'frame $i');
          expect(fast.dots.length, enc.info.dotCount);
        }
        expect(
          debugFrameJsonFallbacks,
          before,
          reason: 'real frames must take the fast path',
        );
      });
    }

    test('a wrong capacity hint still works', () {
      final enc = ParticleEncoder.create(_payload(200));
      addTearDown(enc.dispose);
      final text = enc.frameJson(0, unixSeconds: 5);
      final bytes = Uint8List.fromList(utf8.encode(text));
      for (final hint in [0, 1, 17, 100000]) {
        expect(
          parseFrameJsonBytes(
            bytes,
            dotCountHint: hint,
          ).sameAs(_viaJsonDecode(text)),
          isTrue,
          reason: 'hint $hint',
        );
      }
    });

    test('other layouts fall back to jsonDecode and still parse', () {
      final enc = ParticleEncoder.create(_payload(200));
      addTearDown(enc.dispose);
      final text = enc.frameJson(0, unixSeconds: 5);
      final expected = _viaJsonDecode(text);
      // Same data, different formatting: whitespace everywhere.
      final pretty = const JsonEncoder.withIndent(
        '  ',
      ).convert(jsonDecode(text));
      final before = debugFrameJsonFallbacks;
      final f = parseFrameJsonBytes(Uint8List.fromList(utf8.encode(pretty)));
      expect(debugFrameJsonFallbacks, before + 1);
      expect(f.sameAs(expected), isTrue);

      // Truncated input is an error, not a crash.
      final cut = Uint8List.fromList(utf8.encode(text.substring(0, 5000)));
      expect(() => parseFrameJsonBytes(cut), throwsFormatException);
      expect(
        () => parseFrameJsonBytes(Uint8List.fromList(utf8.encode('{}'))),
        throwsA(anything),
      );
    });
  });

  group('number parsing is exactly double.parse', () {
    test('edge cases', () {
      final cases = <String>[
        '0',
        '0.0',
        '-0.0',
        '1',
        '-1',
        '0.1',
        '0.30000000000000004',
        '-30.31088913245535',
        '123456789012345678',
        '9007199254740992',
        '9007199254740993',
        '0.000001',
        '1e-7',
        '1.5E+3',
        '2.5e22',
        '2.5e23',
        '1e300',
        '1.7976931348623157e308',
        '5e-324',
        '4.9406564584124654e-324',
        '0.1234567890123456789012345',
        '100000000000000000000000.5',
      ];
      final dots = _parseCoords(cases);
      for (var i = 0; i < cases.length; i++) {
        final expected = double.parse(cases[i]);
        expect(
          dots.x[i].toString() + (dots.x[i].isNegative ? '-' : '+'),
          expected.toString() + (expected.isNegative ? '-' : '+'),
          reason: cases[i],
        );
        expect(dots.x[i], expected, reason: cases[i]);
      }
      expect(dots.q.first, -3);
      expect(dots.r[2], -2);
      expect(dots.color.toList().sublist(0, 5), [0, 1, 2, 3, 0]);
      expect(dots.size.toList().sublist(0, 4), [0, 1, 0, 1]);
    });

    test('random doubles round trip bit for bit', () {
      final rnd = math.Random(42);
      final values = <double>[];
      for (var i = 0; i < 30000; i++) {
        final scale = math.pow(10, rnd.nextInt(8) - 4).toDouble();
        values.add((rnd.nextDouble() - 0.5) * 100 * scale);
      }
      // serde-like computed coordinates: sums and products of small numbers.
      for (var i = 0; i < 10000; i++) {
        values.add((i - 5000) * 0.8660254037844386 + rnd.nextInt(97) / 2);
      }
      final dots = _parseCoords([for (final v in values) v.toString()]);
      for (var i = 0; i < values.length; i++) {
        expect(
          dots.x[i].compareTo(values[i]),
          0,
          reason: 'value ${values[i]} parsed as ${dots.x[i]}',
        );
      }
      expect(dots.length, values.length);
    });

    test('malformed dots are rejected, never panics', () {
      for (final bad in [
        '[{"q":1}]',
        '[{"q":1,"r":2,"at":{"x":1,"y":2},"color":0,"size":0]',
        '[{"q":x,"r":2,"at":{"x":1,"y":2},"color":0,"size":0}]',
        '[{"q":1,"r":2,"at":{"x":-,"y":2},"color":0,"size":0}]',
        '[{"q":1,"r":2,"at":{"x":1e,"y":2},"color":0,"size":0}]',
        '[{"q":1,"r":2,"at":{"x":1,"y":2},"color":0,"size":0}',
        '[{"q":1,"r":2,"at":{"x":1,"y":2},"color":0,"size":0};',
        '[',
      ]) {
        expect(
          () => parseDotsArray(Uint8List.fromList(utf8.encode(bad)), 1),
          throwsA(anyOf(isA<FormatException>(), isA<RangeError>())),
          reason: bad,
        );
      }
    });
  });
}
