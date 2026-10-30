// Parity of lib/src/motion.dart with `particlecode_core::motion`.
//
// test/fixtures/motion_reference.json holds values computed by the Rust code; the
// generator is test/fixtures/gen_motion_reference.rs (a `main.rs` for a scratch crate
// that depends on `particlecode-core` by path; regenerate with `cargo run --release`
// and redirect stdout). 800 breathing / flow-field dot
// samples, 120 heartbeat samples, 200 reveal and ripple samples and 300 particle poses,
// over seeds that exercise the full 64-bit range.
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';

/// Parses a 16-digit hex seed as the bit pattern of a Rust u64.
int _seed(String hex) => BigInt.parse(hex, radix: 16).toSigned(64).toInt();

// sin/cos come from libm in Rust and from the C library in Dart; they agree to a few
// ulps, which grows with the argument (times up to 900 s).
const _tol = 1e-9;

void main() {
  final json =
      jsonDecode(File('test/fixtures/motion_reference.json').readAsStringSync())
          as Map<String, Object?>;
  List<Map<String, Object?>> rows(String key) =>
      (json[key]! as List<Object?>).cast<Map<String, Object?>>();
  double d(Object? v) => (v! as num).toDouble();

  test('seed_from_session is bit-exact', () {
    for (final r in rows('seeds')) {
      expect(
        seedFromSession(r['session']! as int),
        _seed(r['seed']! as String),
        reason: 'session ${r['session']}',
      );
    }
  });

  group('dots', () {
    for (final mood in ['Breathing', 'FlowField']) {
      test('$mood offset and radius scale', () {
        final cfg = MotionConfig(
          mood: mood == 'Breathing' ? Mood.breathing : Mood.flowField,
        );
        var n = 0;
        var maxErr = 0.0;
        for (final r in rows('dots').where((r) => r['mood'] == mood)) {
          final seed = _seed(r['seed']! as String);
          final t = d(r['t']);
          final x = d(r['x']);
          final y = d(r['y']);
          final half = d(r['half']);
          final o = dotOffset(cfg, seed, t, x, y, half);
          final s = dotRadiusScale(cfg, seed, t, x, y, half);
          maxErr = [
            maxErr,
            (o.dx - d(r['ox'])).abs(),
            (o.dy - d(r['oy'])).abs(),
            (s - d(r['scale'])).abs(),
          ].reduce(math.max);
          expect(o.dx, closeTo(d(r['ox']), _tol), reason: '$r');
          expect(o.dy, closeTo(d(r['oy']), _tol), reason: '$r');
          expect(s, closeTo(d(r['scale']), _tol), reason: '$r');
          n++;
        }
        expect(n, 400);
        // ignore: avoid_print
        print('$mood: $n samples, max abs error $maxErr');
      });
    }

    test('DotMotion.sample equals the top-level functions', () {
      final cfg = const MotionConfig(mood: Mood.flowField);
      final out = Float64List(3);
      final m = DotMotion(cfg, 99, 12.5, 38);
      m.sample(3.5, -7.25, out);
      final o = dotOffset(cfg, 99, 12.5, 3.5, -7.25, 38);
      expect(out[0], o.dx);
      expect(out[1], o.dy);
      expect(out[2], dotRadiusScale(cfg, 99, 12.5, 3.5, -7.25, 38));
    });

    test('offsets stay inside the format limit', () {
      for (final mood in Mood.values) {
        final cfg = MotionConfig(mood: mood);
        for (var i = 0; i < 2000; i++) {
          final x = (i * 0.37) % 50 - 25;
          final y = (i * 0.91) % 50 - 25;
          final o = dotOffset(cfg, 4660, i * 0.13, x, y, 38);
          expect(
            math.sqrt(o.dx * o.dx + o.dy * o.dy),
            lessThanOrEqualTo(maxDrift),
          );
          expect(
            dotRadiusScale(cfg, 4660, i * 0.13, x, y, 38),
            inInclusiveRange(0.95, 1.05),
          );
        }
      }
    });
  });

  test('anchor_scale', () {
    for (final r in rows('anchor_scale')) {
      final cfg = MotionConfig(anchorHeartbeat: r['heartbeat']! as bool);
      expect(
        anchorScale(cfg, d(r['t']), r['index']! as int),
        closeTo(d(r['v']), _tol),
        reason: '$r',
      );
    }
  });

  test('crossfade_alpha is exact', () {
    for (final r
        in (json['crossfade']! as List<Object?>).cast<Map<String, Object?>>()) {
      expect(crossfadeAlpha(d(r['t'])), d(r['v']), reason: '$r');
    }
  });

  test('reveal_opacity and ripple_lighten', () {
    for (final r in rows('reveal_ripple')) {
      final t = d(r['t']);
      final x = d(r['x']);
      final y = d(r['y']);
      final half = d(r['half']);
      expect(
        revealOpacity(t, x, y, half),
        closeTo(d(r['reveal']), _tol),
        reason: '$r',
      );
      expect(
        rippleLighten(t, x, y, half),
        closeTo(d(r['ripple']), _tol),
        reason: '$r',
      );
    }
  });

  test('particle_pose', () {
    var n = 0;
    for (final r in rows('particles')) {
      final band = ParticleBand(inner: d(r['inner']), outer: d(r['outer']));
      final pose = particlePose(
        _seed(r['seed']! as String),
        d(r['t']),
        r['k']! as int,
        d(r['from']),
        d(r['to']),
        d(r['since']),
        band,
      );
      expect(pose.angle, closeTo(d(r['angle']), _tol), reason: '$r');
      expect(pose.radius, closeTo(d(r['radius']), _tol), reason: '$r');
      n++;
    }
    expect(n, 300);
  });

  test('motion config defaults and all()', () {
    const def = MotionConfig();
    expect(def.mood, Mood.breathing);
    expect([
      def.ripple,
      def.anchorHeartbeat,
      def.particleTrails,
      def.reveal,
      def.shimmer,
    ], everyElement(isFalse));
    const all = MotionConfig.all();
    expect([
      all.ripple,
      all.anchorHeartbeat,
      all.particleTrails,
      all.reveal,
      all.shimmer,
    ], everyElement(isTrue));
    expect(def.copyWith(ripple: true).ripple, isTrue);
    expect(const MotionConfig(), def);
  });
}
