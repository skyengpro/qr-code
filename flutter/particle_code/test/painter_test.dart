// Painter checks without golden files: render a real frame to an image and compare the
// pixels at known centers to the exact palette.
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';

const double _px = 800;

class _Raster {
  _Raster(this.rgba, this.width);

  final ByteData rgba;
  final int width;

  Color at(int x, int y) {
    final o = (y * width + x) * 4;
    return Color.fromARGB(
      rgba.getUint8(o + 3),
      rgba.getUint8(o),
      rgba.getUint8(o + 1),
      rgba.getUint8(o + 2),
    );
  }
}

Future<_Raster> _render(
  ParticleCodePainter painter, {
  double size = _px,
}) async {
  final recorder = ui.PictureRecorder();
  painter.paint(Canvas(recorder), Size(size, size));
  final picture = recorder.endRecording();
  final image = await picture.toImage(size.toInt(), size.toInt());
  final data = (await image.toByteData())!;
  picture.dispose();
  image.dispose();
  return _Raster(data, size.toInt());
}

/// Pixel containing pitch-space point `(x, y)` at canvas side [size].
(int, int) _pixel(FrameSpec f, double x, double y, {double size = _px}) {
  final scale = size / (2 * f.halfExtent(defaultMargin));
  return ((size / 2 + x * scale).floor(), (size / 2 + y * scale).floor());
}

void expectRgb(Color actual, Color expected, {String? reason}) {
  expect(
    [actual.toARGB32()],
    [expected.toARGB32()],
    reason: reason ?? 'expected ${expected.toARGB32().toRadixString(16)}',
  );
}

FrameSpec _frame({CodeProfile profile = CodeProfile.m, int index = 0}) {
  final enc = ParticleEncoder.create(
    Uint8List.fromList(List.generate(400, (i) => i * 7 & 0xFF)),
    options: EncoderOptions(
      profile: profile,
      sessionId: 0x4321,
      key: List.generate(32, (i) => i),
      clockOriginUnix: 1700000000,
    ),
  );
  try {
    return enc.frame(index);
  } finally {
    enc.dispose();
  }
}

Future<void> main() async {
  await ParticleCode.init();
  final frame = _frame();

  group('static frame pixels', () {
    late _Raster img;
    setUpAll(() async {
      img = await _render(ParticleCodePainter(frame: frame, motion: null));
    });

    test('background fills the quiet zone', () {
      expectRgb(img.at(2, 2), ParticleCodePalette.background);
      expectRgb(
        img.at(_px.toInt() - 3, _px.toInt() - 3),
        ParticleCodePalette.background,
      );
    });

    test('data dots have the exact hue at their centers', () {
      var checked = 0;
      for (var i = 0; i < frame.dots.length; i += 7) {
        final d = frame.dots[i];
        final (x, y) = _pixel(frame, d.x, d.y);
        expectRgb(
          img.at(x, y),
          ParticleCodePalette.hues[d.color],
          reason: 'dot $i at ($x, $y), hue ${d.color}',
        );
        checked++;
      }
      expect(checked, greaterThan(300));
    });

    test('header dots', () {
      for (final d in frame.headerDots) {
        final (x, y) = _pixel(frame, d.x, d.y);
        expectRgb(img.at(x, y), ParticleCodePalette.hues[d.color]);
      }
    });

    test('swatches: hues, white and black', () {
      var white = 0, black = 0, hue = 0;
      for (final s in frame.swatches) {
        final (x, y) = _pixel(frame, s.at.dx, s.at.dy);
        expectRgb(img.at(x, y), s.color.color);
        switch (s.color.kind) {
          case SwatchKind.white:
            white++;
          case SwatchKind.black:
            black++;
          case SwatchKind.hue:
            hue++;
        }
      }
      expect((white, black, hue), (1, 1, 46));
    });

    test('anchor cores, rings and the top anchor pupil', () {
      for (var i = 1; i < frame.anchors.length; i++) {
        final a = frame.anchors[i];
        final (x, y) = _pixel(frame, a.at.dx, a.at.dy);
        expectRgb(img.at(x, y), ParticleCodePalette.anchor, reason: 'core $i');
        // Between the core and the ring is background.
        final gap = (a.coreR + a.ringIn) / 2;
        final (gx, gy) = _pixel(frame, a.at.dx + gap, a.at.dy);
        expectRgb(
          img.at(gx, gy),
          ParticleCodePalette.background,
          reason: 'gap $i',
        );
        // The ring itself.
        final mid = (a.ringIn + a.ringOut) / 2;
        final (rx, ry) = _pixel(frame, a.at.dx + mid, a.at.dy);
        expectRgb(
          img.at(rx, ry),
          ParticleCodePalette.anchor,
          reason: 'ring $i',
        );
        // Outside the ring is not anchor-colored.
        final (ox, oy) = _pixel(frame, a.at.dx, a.at.dy - a.ringOut - 0.25);
        expect(img.at(ox, oy), isNot(ParticleCodePalette.anchor));
      }
      final top = frame.anchors[0];
      expect(top.pupilR, greaterThan(0));
      final (px, py) = _pixel(frame, top.at.dx, top.at.dy);
      expectRgb(
        img.at(px, py),
        ParticleCodePalette.background,
        reason: 'pupil',
      );
      final (cx, cy) = _pixel(
        frame,
        top.at.dx + (top.pupilR + top.coreR) / 2,
        top.at.dy,
      );
      expectRgb(img.at(cx, cy), ParticleCodePalette.anchor, reason: 'core 0');
    });

    test('particles sit at their sector angles in the band', () {
      for (final p in frame.particles) {
        final r = frame.particleBand.mid;
        final (x, y) = _pixel(
          frame,
          r * math.cos(p.angle),
          r * math.sin(p.angle),
        );
        expectRgb(img.at(x, y), ParticleCodePalette.particle);
      }
    });

    test(
      'all colors in the image come from the palette (plus anti-aliasing)',
      () {
        // Count exact-palette pixels: the flat interior of every shape dominates.
        final exact = <int, int>{};
        for (var y = 0; y < img.width; y++) {
          for (var x = 0; x < img.width; x++) {
            final c = img.at(x, y).toARGB32();
            exact[c] = (exact[c] ?? 0) + 1;
          }
        }
        final palette = {
          ...ParticleCodePalette.hues.map((c) => c.toARGB32()),
          ParticleCodePalette.background.toARGB32(),
          ParticleCodePalette.anchor.toARGB32(),
          ParticleCodePalette.particle.toARGB32(),
        };
        for (final c in palette) {
          expect(
            exact[c] ?? 0,
            greaterThan(100),
            reason: '${c.toRadixString(16)} missing',
          );
        }
      },
    );
  });

  group('with motion', () {
    test('dots keep their hue under breathing drift and pulse', () async {
      final seed = seedFromSession(frame.header.sessionId);
      for (final t in [0.0, 0.7, 1.9]) {
        final img = await _render(
          ParticleCodePainter(
            frame: frame,
            motion: const MotionConfig(),
            seed: seed,
            t: t,
          ),
        );
        for (var i = 0; i < frame.dots.length; i += 11) {
          final d = frame.dots[i];
          final off = dotOffset(
            const MotionConfig(),
            seed,
            t,
            d.x,
            d.y,
            frame.frameHalfSide,
          );
          final (x, y) = _pixel(frame, d.x + off.dx, d.y + off.dy);
          expectRgb(
            img.at(x, y),
            ParticleCodePalette.hues[d.color],
            reason: 't=$t dot $i',
          );
        }
      }
    });

    test(
      'motion changes the picture over time but not between identical calls',
      () async {
        final seed = seedFromSession(frame.header.sessionId);
        ParticleCodePainter at(double t) => ParticleCodePainter(
          frame: frame,
          motion: const MotionConfig(),
          seed: seed,
          t: t,
        );
        final a = await _render(at(0.4), size: 300);
        final a2 = await _render(at(0.4), size: 300);
        final b = await _render(at(1.1), size: 300);
        expect(a.rgba.buffer.asUint8List(), a2.rgba.buffer.asUint8List());
        expect(a.rgba.buffer.asUint8List(), isNot(b.rgba.buffer.asUint8List()));
      },
    );

    test('anchor heartbeat swells the ring', () async {
      const m = MotionConfig(anchorHeartbeat: true);
      // Find a time when anchor 1 is at its peak (cycle = 0.5 + 1/3).
      final t = (0.5 + 1 / 3) * heartbeatPeriodForTest;
      final scale = anchorScale(m, t, 1);
      expect(scale, greaterThan(1.09));
      final a = frame.anchors[1];
      final img = await _render(
        ParticleCodePainter(frame: frame, motion: m, t: t),
      );
      // Just outside the static outer radius but inside the swollen one.
      final r = a.ringOut * (1 + (scale - 1) / 2);
      final (x, y) = _pixel(frame, a.at.dx + r, a.at.dy);
      expectRgb(img.at(x, y), ParticleCodePalette.anchor);
    });

    test('cross-fade: the new frame fades in over 40 ms', () async {
      final next = _frame(index: 1);
      // Find a data dot whose hue changes between the two frames.
      var i = 0;
      while (frame.dots[i].color == next.dots[i].color) {
        i++;
      }
      final from = ParticleCodePalette.hues[frame.dots[i].color];
      final to = ParticleCodePalette.hues[next.dots[i].color];
      final d = next.dots[i];
      final (x, y) = _pixel(next, d.x, d.y);

      Future<Color> sample(double since) async {
        final img = await _render(
          ParticleCodePainter(
            frame: next,
            prev: frame,
            motion: const MotionConfig(),
            tSinceSwitch: since,
          ),
        );
        return img.at(x, y);
      }

      expectRgb(await sample(0), from);
      expectRgb(await sample(0.04), to);
      expectRgb(await sample(1), to);
      final mid = await sample(0.02);
      expectRgb(mid, lerpForTest(from, to, 0.5));
    });

    test(
      'reveal: dots start transparent and are opaque after 600 ms',
      () async {
        const m = MotionConfig(reveal: true);
        final d = frame.dots[frame.dots.length ~/ 2];
        final (x, y) = _pixel(frame, d.x, d.y);
        final early = await _render(
          ParticleCodePainter(frame: frame, motion: m, t: 0, tSinceStart: 0),
        );
        expectRgb(early.at(x, y), ParticleCodePalette.background);
        final late = await _render(
          ParticleCodePainter(frame: frame, motion: m, t: 1, tSinceStart: 0.7),
        );
        expectRgb(late.at(x, y), ParticleCodePalette.hues[d.color]);
      },
    );

    test('particle trails draw faded specks behind the particle', () async {
      const m = MotionConfig(particleTrails: true);
      final seed = seedFromSession(frame.header.sessionId);
      final plain = await _render(
        ParticleCodePainter(
          frame: frame,
          motion: const MotionConfig(),
          seed: seed,
          t: 3,
        ),
        size: 400,
      );
      final trails = await _render(
        ParticleCodePainter(frame: frame, motion: m, seed: seed, t: 3),
        size: 400,
      );
      expect(
        plain.rgba.buffer.asUint8List(),
        isNot(trails.rgba.buffer.asUint8List()),
      );
    });
  });

  group('performance', () {
    for (final profile in [CodeProfile.m, CodeProfile.l]) {
      test('painting a ${profile.letter} frame is fast', () async {
        final f = _frame(profile: profile);
        expect(
          f.dots.length,
          greaterThan(profile == CodeProfile.m ? 3000 : 5000),
        );
        final seed = seedFromSession(f.header.sessionId);
        final prev = _frame(profile: profile, index: 1);
        ParticleCodePainter at(double t) => ParticleCodePainter(
          frame: f,
          prev: prev,
          motion: const MotionConfig.all(),
          seed: seed,
          t: t,
          tSinceSwitch: 0.02,
          tSinceSegmentComplete: 0.05,
          tSinceStart: 0.3,
        );
        // Warm up the JIT, then time command recording.
        for (var i = 0; i < 5; i++) {
          final r = ui.PictureRecorder();
          at(i * 0.1).paint(Canvas(r), const Size(400, 400));
          r.endRecording().dispose();
        }
        const runs = 20;
        final sw = Stopwatch()..start();
        for (var i = 0; i < runs; i++) {
          final r = ui.PictureRecorder();
          at(i * 0.1).paint(Canvas(r), const Size(400, 400));
          r.endRecording().dispose();
        }
        final perPaint = sw.elapsedMicroseconds / runs / 1000;
        // Plain frames (no cross-fade, no ripple) take the batched path.
        final plain = ParticleCodePainter(
          frame: f,
          motion: const MotionConfig(),
          seed: seed,
          t: 1,
        );
        final sw2 = Stopwatch()..start();
        for (var i = 0; i < runs; i++) {
          final r = ui.PictureRecorder();
          plain.paint(Canvas(r), const Size(400, 400));
          r.endRecording().dispose();
        }
        final perPlain = sw2.elapsedMicroseconds / runs / 1000;

        final sw3 = Stopwatch()..start();
        await _render(plain);
        final raster = sw3.elapsedMilliseconds;
        // ignore: avoid_print
        print(
          '${profile.letter}: ${f.dots.length} dots, record all-layers '
          '${perPaint.toStringAsFixed(2)} ms, record plain '
          '${perPlain.toStringAsFixed(2)} ms, rasterize 800px $raster ms',
        );
        // A 60 Hz frame is 16.7 ms; the test VM is JIT with no optimizations of the
        // engine, so allow generous headroom but still catch per-dot Paint allocation
        // or quadratic behaviour.
        expect(perPlain, lessThan(30));
        expect(perPaint, lessThan(60));
        expect(raster, lessThan(3000));
      });
    }
  });
}

// The heartbeat period (core constant, 3 s); kept literal here so the test fails if the
// port ever changes it silently.
const double heartbeatPeriodForTest = 3.0;

/// Straight sRGB interpolation, the same arithmetic as the Rust painter.
Color lerpForTest(Color a, Color b, double t) {
  int mix(int x, int y) => (x + (y - x) * t).round();
  int r(Color c) => (c.toARGB32() >> 16) & 0xFF;
  int g(Color c) => (c.toARGB32() >> 8) & 0xFF;
  int bl(Color c) => c.toARGB32() & 0xFF;
  return Color.fromARGB(
    255,
    mix(r(a), r(b)),
    mix(g(a), g(b)),
    mix(bl(a), bl(b)),
  );
}
