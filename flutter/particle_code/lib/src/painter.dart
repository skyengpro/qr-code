import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/rendering.dart';

import 'frame_spec.dart';
import 'motion.dart' as mo;
import 'motion.dart' show MotionConfig;
import 'palette.dart';

/// Number of fading discs in a particle trail.
const int _trailSteps = 8;

/// Default quiet zone around the frame, in pitches (the Rust renderer's default).
const double defaultMargin = 2.5;

/// Paints a [FrameSpec] exactly like the Rust reference painter
/// (`particlecode-render`): background, anchors, swatches, header dots, data dots,
/// particles, back to front, in the exact palette.
///
/// With [motion] set to `null` the frame is drawn static (dots on their lattice points,
/// particles at their encoded angles). Otherwise the animation layers of [motion] are
/// evaluated at session time [t]. All animation is a pure function of the arguments, so
/// a dropped paint never drifts the field.
///
/// The code is scaled so `[-(R + margin), R + margin]^2` fits the shorter side of the
/// canvas and is centered; the background fills the whole canvas.
class ParticleCodePainter extends CustomPainter {
  /// Creates a painter for [frame].
  const ParticleCodePainter({
    required this.frame,
    this.prev,
    this.motion = const MotionConfig(),
    this.seed = 0,
    this.t = 0,
    this.tSinceSwitch = double.infinity,
    this.tSinceSegmentComplete,
    this.tSinceStart,
    this.background = ParticleCodePalette.background,
    this.margin = defaultMargin,
  });

  /// The data frame to draw.
  final FrameSpec frame;

  /// The data frame shown before [frame], for the cross-fade and the particle glide.
  final FrameSpec? prev;

  /// Which animation layers are applied; `null` draws the frame static.
  final MotionConfig? motion;

  /// Session motion seed (`seedFromSession(sessionId)`).
  final int seed;

  /// Seconds since the session started; drives drift, heartbeat and particle wobble.
  final double t;

  /// Seconds since the switch from [prev] to [frame].
  final double tSinceSwitch;

  /// Seconds since the last completed segment, for the ripple (`null` = no ripple now).
  final double? tSinceSegmentComplete;

  /// Seconds since the reveal started (`null` = fully revealed).
  final double? tSinceStart;

  /// Background and quiet-zone color; also the color of the top anchor's pupil.
  final Color background;

  /// Quiet zone around `[-R, R]^2`, in pitches. Values below 1.5 are raised to it.
  final double margin;

  @override
  void paint(Canvas canvas, Size size) {
    final side = math.min(size.width, size.height);
    final paintRect = Offset.zero & size;
    canvas.drawRect(paintRect, Paint()..color = background);
    if (!(side > 0)) return;

    final half = frame.halfExtent(margin);
    final scale = side / (2 * half);
    canvas.save();
    canvas.clipRect(paintRect);
    canvas.translate(size.width / 2, size.height / 2);
    canvas.scale(scale);

    final painter = _Brush(canvas);
    _drawAnchors(painter);
    _drawSwatches(painter);
    _drawDots(painter, frame.headerDots, prev?.headerDots, lattice: false);
    _drawDots(painter, frame.dots, prev?.dots, lattice: true);
    _drawParticles(painter);
    canvas.restore();
  }

  @override
  bool shouldRepaint(ParticleCodePainter old) =>
      old.frame != frame ||
      old.prev != prev ||
      old.motion != motion ||
      old.seed != seed ||
      old.t != t ||
      old.tSinceSwitch != tSinceSwitch ||
      old.tSinceSegmentComplete != tSinceSegmentComplete ||
      old.tSinceStart != tSinceStart ||
      old.background != background ||
      old.margin != margin;

  // -------------------------------------------------------------------------------------

  double _revealAlpha(double x, double y) {
    final m = motion;
    final ts = tSinceStart;
    if (m == null || !m.reveal || ts == null) return 1.0;
    return mo.revealOpacity(ts, x, y, frame.frameHalfSide);
  }

  void _drawAnchors(_Brush b) {
    for (var i = 0; i < frame.anchors.length; i++) {
      final a = frame.anchors[i];
      final m = motion;
      final s = m == null ? 1.0 : mo.anchorScale(m, t, i);
      b.ring(a.at, a.ringIn * s, a.ringOut * s, ParticleCodePalette.anchor);
      b.disc(a.at.dx, a.at.dy, a.coreR * s, ParticleCodePalette.anchor, 1.0);
      if (a.pupilR > 0) {
        b.disc(a.at.dx, a.at.dy, a.pupilR * s, background, 1.0);
      }
    }
  }

  void _drawSwatches(_Brush b) {
    for (final s in frame.swatches) {
      b.disc(
        s.at.dx,
        s.at.dy,
        CodeDims.swatchDotR,
        s.color.color,
        _revealAlpha(s.at.dx, s.at.dy),
      );
    }
  }

  /// Header dots (`lattice == false`) are rigid: their size references are the static
  /// swatches. Payload dots drift, share a radius pulse and take the ripple.
  void _drawDots(
    _Brush b,
    List<FrameDot> dots,
    List<FrameDot>? prevDots, {
    required bool lattice,
  }) {
    if (dots.isEmpty) return;
    final m = motion;
    final halfSide = frame.frameHalfSide;
    final fade = (m == null || prev == null)
        ? 1.0
        : mo.crossfadeAlpha(tSinceSwitch);
    final comparable =
        prev != null &&
        prev!.profile == frame.profile &&
        prevDots != null &&
        prevDots.length == dots.length;
    final shimmer = m?.shimmer ?? false;
    final ripple = (m?.ripple ?? false) ? tSinceSegmentComplete : null;
    final dotMotion = (m != null && lattice)
        ? mo.DotMotion(m, seed, t, halfSide)
        : null;
    final sampled = Float64List(3);

    final paths = lattice
        ? List<Path>.generate(
            ParticleCodePalette.hues.length,
            (_) => Path(),
            growable: false,
          )
        : null;

    for (var i = 0; i < dots.length; i++) {
      final d = dots[i];
      final colorIdx = d.color & 3;
      var rgb = ParticleCodePalette.hues[colorIdx];
      var radius = d.radius;
      var plain = true;

      if (fade < 1.0 && comparable) {
        final p = prevDots[i];
        if (p.q == d.q && p.r == d.r) {
          if (p.color != d.color) {
            final from = ParticleCodePalette.hues[p.color & 3];
            rgb = shimmer ? lerpHue(from, rgb, fade) : lerpRgb(from, rgb, fade);
            plain = false;
          }
          radius = p.radius + (radius - p.radius) * fade;
        }
      }

      var x = d.x;
      var y = d.y;
      if (dotMotion != null) {
        dotMotion.sample(d.x, d.y, sampled);
        x = d.x + sampled[0];
        y = d.y + sampled[1];
        radius *= sampled[2];
        if (ripple != null) {
          final amount = mo.rippleLighten(ripple, d.x, d.y, halfSide);
          if (amount > 0) {
            rgb = lighten(rgb, amount);
            plain = false;
          }
        }
      }
      final alpha = m == null ? 1.0 : _revealAlpha(d.x, d.y);
      if (alpha < 1.0) plain = false;

      if (plain && paths != null) {
        paths[colorIdx].addOval(
          Rect.fromCircle(center: Offset(x, y), radius: radius),
        );
      } else {
        b.disc(x, y, radius, rgb, alpha);
      }
    }

    if (paths != null) {
      for (var c = 0; c < paths.length; c++) {
        b.fillPath(paths[c], ParticleCodePalette.hues[c]);
      }
    }
  }

  void _drawParticles(_Brush b) {
    final band = frame.particleBand;
    final m = motion;
    for (var k = 0; k < frame.particles.length; k++) {
      final p = frame.particles[k];
      if (m == null) {
        b.disc(
          band.mid * math.cos(p.angle),
          band.mid * math.sin(p.angle),
          CodeDims.particleR,
          ParticleCodePalette.particle,
          1.0,
        );
        continue;
      }
      final pf = prev;
      final from = (pf != null && k < pf.particles.length)
          ? pf.particles[k].angle
          : p.angle;
      ({double angle, double radius}) pose(double dt) => mo.particlePose(
        seed,
        t - dt,
        k,
        from,
        p.angle,
        tSinceSwitch - dt,
        band,
      );
      final now = pose(0.0);
      if (m.particleTrails) {
        final before = pose(0.02);
        final sign = now.angle >= before.angle ? 1.0 : -1.0;
        final span = mo.trailLength / math.max(now.radius, 1.0);
        for (var s = _trailSteps; s >= 1; s--) {
          final f = s / _trailSteps;
          final a = now.angle - sign * span * f;
          b.disc(
            now.radius * math.cos(a),
            now.radius * math.sin(a),
            CodeDims.particleR * (1.0 - 0.45 * f),
            ParticleCodePalette.particle,
            0.55 * (1.0 - f),
          );
        }
      }
      b.disc(
        now.radius * math.cos(now.angle),
        now.radius * math.sin(now.angle),
        CodeDims.particleR,
        ParticleCodePalette.particle,
        1.0,
      );
    }
  }
}

/// A reused [Paint] with pitch-unit drawing helpers.
class _Brush {
  _Brush(this.canvas);

  final Canvas canvas;
  final Paint _paint = Paint()..isAntiAlias = true;

  void _set(Color c, double alpha) {
    final a = (alpha.clamp(0.0, 1.0) * 255.0).round();
    _paint.color = a == 255 ? c : c.withAlpha(a);
  }

  /// Filled circle: exact flat color, anti-aliased edge.
  void disc(double x, double y, double r, Color c, double alpha) {
    if (r <= 0 || alpha <= 0) return;
    _set(c, alpha);
    canvas.drawCircle(Offset(x, y), r, _paint);
  }

  /// Annulus between [rIn] and [rOut] (even-odd, so the hole is clear).
  void ring(Offset at, double rIn, double rOut, Color c) {
    if (rOut <= rIn || rOut <= 0) return;
    final path = Path()
      ..fillType = PathFillType.evenOdd
      ..addOval(Rect.fromCircle(center: at, radius: rOut))
      ..addOval(Rect.fromCircle(center: at, radius: math.max(rIn, 0)));
    _set(c, 1.0);
    canvas.drawPath(path, _paint);
  }

  void fillPath(Path path, Color c) {
    _set(c, 1.0);
    canvas.drawPath(path, _paint);
  }
}
