// A faithful Dart port of `particlecode_core::motion`.
//
// Every painter (this one, the web painter, the Rust GIF packager) evaluates the same
// pure functions of (t, session seed). The formulas, constants and operation order follow
// the Rust source so the results agree to floating point rounding; the parity tests in
// test/motion_parity_test.dart compare against values computed by the Rust code.
//
// The hashing uses 64-bit unsigned arithmetic. Dart's VM `int` is a signed 64-bit value
// whose `*`, `+` and `^` wrap exactly like Rust's `wrapping_*`, so only the right shifts
// need care: they use `>>>` (logical) instead of `>>`. This file therefore needs the Dart
// VM (mobile, desktop); it is not meant for dart2js.

import 'dart:math' as math;
import 'dart:typed_data';

import 'frame_spec.dart';

/// Largest distance a dot may drift from its lattice point, in pitches.
const double maxDrift = 0.12;

/// Drift amplitude actually used (a hair below [maxDrift]).
const double _driftAmplitude = 0.115;

/// Relative dot radius pulse (+-5%).
const double radiusPulse = 0.05;

/// Period of the breathing wave in seconds.
const double breathPeriod = 2.5;

/// Wavelength of the breathing wave as a fraction of the frame half-side `R`.
const double breathWavelengthFrac = 0.6;

/// Largest extra scale of a hero anchor ring under the heartbeat (+10%).
const double heartbeatSwell = 0.10;

/// Period of the anchor heartbeat in seconds.
const double heartbeatPeriod = 3.0;

/// Peak lightening of a dot on the ripple ring (15%).
const double rippleStrength = 0.15;

/// Duration of the ripple in seconds.
const double rippleDuration = 0.3;

/// Duration of the reveal in seconds.
const double revealDuration = 0.6;

/// Duration of the colour cross-fade at a data-frame switch, in seconds.
const double crossfadeDuration = 0.04;

/// Duration of the particle glide at a data-frame switch, in seconds.
const double particleGlide = 0.06;

/// Largest particle wobble around its sector centre, in radians (0.8 degrees).
const double particleWobble = 0.8 * math.pi / 180.0;

/// Length of a particle trail, in pitches.
const double trailLength = 0.8;

/// Interval of the data-frame timer at 10 fps, in seconds. Use `1 / fps` for other rates.
const double dataFrameInterval = 0.1;

/// How dots drift around their lattice points.
enum Mood {
  /// A slow sine wave travelling across the field (the default).
  breathing,

  /// A smooth seeded noise field: wind through grass.
  flowField,
}

/// Which animation layers a painter applies. The default is the spec's default: the
/// breathing wave and nothing else.
class MotionConfig {
  /// Creates a config.
  const MotionConfig({
    this.mood = Mood.breathing,
    this.ripple = false,
    this.anchorHeartbeat = false,
    this.particleTrails = false,
    this.reveal = false,
    this.shimmer = false,
  });

  /// Every optional layer switched on, breathing mood.
  const MotionConfig.all()
    : mood = Mood.breathing,
      ripple = true,
      anchorHeartbeat = true,
      particleTrails = true,
      reveal = true,
      shimmer = true;

  /// Dot drift and radius pulse.
  final Mood mood;

  /// A ring of brightness on every completed segment.
  final bool ripple;

  /// The three hero rings swell in a slow, offset pulse.
  final bool anchorHeartbeat;

  /// Particles leave a fading tail.
  final bool particleTrails;

  /// Dots fade in from the centre at session start.
  final bool reveal;

  /// Hue-rotating (instead of straight RGB) cross-fade at a data-frame switch.
  final bool shimmer;

  /// A copy with some fields replaced.
  MotionConfig copyWith({
    Mood? mood,
    bool? ripple,
    bool? anchorHeartbeat,
    bool? particleTrails,
    bool? reveal,
    bool? shimmer,
  }) => MotionConfig(
    mood: mood ?? this.mood,
    ripple: ripple ?? this.ripple,
    anchorHeartbeat: anchorHeartbeat ?? this.anchorHeartbeat,
    particleTrails: particleTrails ?? this.particleTrails,
    reveal: reveal ?? this.reveal,
    shimmer: shimmer ?? this.shimmer,
  );

  @override
  bool operator ==(Object other) =>
      other is MotionConfig &&
      other.mood == mood &&
      other.ripple == ripple &&
      other.anchorHeartbeat == anchorHeartbeat &&
      other.particleTrails == particleTrails &&
      other.reveal == reveal &&
      other.shimmer == shimmer;

  @override
  int get hashCode => Object.hash(
    mood,
    ripple,
    anchorHeartbeat,
    particleTrails,
    reveal,
    shimmer,
  );
}

// ---------------------------------------------------------------------------------------
// Hashing and noise
// ---------------------------------------------------------------------------------------

/// SplitMix64 finaliser (`mix` in the Rust source).
int _mix(int z) {
  z = z + 0x9E3779B97F4A7C15;
  z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9;
  z = (z ^ (z >>> 27)) * 0x94D049BB133111EB;
  return z ^ (z >>> 31);
}

/// Uniform value in `[0, 1)` derived from `(seed, salt)`.
double _unit(int seed, int salt) =>
    (_mix(seed ^ _mix(salt)) >>> 11).toDouble() / 9007199254740992.0; // 2^53

/// The motion seed every painter should use for a session id, so the app, the GIF
/// packager and the web painter agree on direction and phase.
///
/// The result is the bit pattern of a Rust `u64`; Dart shows it as a signed `int`.
int seedFromSession(int sessionId) =>
    _mix(0x50434F4445000000 | (sessionId & 0xFFFF));

double _smoothstep(double x) {
  final c = x.clamp(0.0, 1.0);
  return c * c * (3.0 - 2.0 * c);
}

const double _f2 = 0.3660254037844386; // (sqrt(3) - 1) / 2
const double _g2 = 0.2113248654051871; // (3 - sqrt(3)) / 6
const double _r = 0.7071067811865476; // FRAC_1_SQRT_2
const List<double> _gradX = [1.0, _r, 0.0, -_r, -1.0, -_r, 0.0, _r];
const List<double> _gradY = [0.0, _r, 1.0, _r, 0.0, -_r, -1.0, -_r];

double _corner(int seed, double ci, double cj, double dx, double dy) {
  final falloff = 0.5 - dx * dx - dy * dy;
  if (falloff <= 0.0) return 0.0;
  final h = _mix(seed ^ _mix(ci.toInt() ^ _mix(cj.toInt())));
  final g = h & 7;
  final f2 = falloff * falloff;
  return f2 * f2 * (_gradX[g] * dx + _gradY[g] * dy);
}

/// 2-D simplex noise with hashed gradients, range roughly `[-1, 1]`.
double _simplex2(int seed, double x, double y) {
  final s = (x + y) * _f2;
  final i = (x + s).floorToDouble();
  final j = (y + s).floorToDouble();
  final t = (i + j) * _g2;
  final x0 = x - (i - t);
  final y0 = y - (j - t);
  final double i1, j1;
  if (x0 > y0) {
    i1 = 1.0;
    j1 = 0.0;
  } else {
    i1 = 0.0;
    j1 = 1.0;
  }
  final x1 = x0 - i1 + _g2;
  final y1 = y0 - j1 + _g2;
  final x2 = x0 - 1.0 + 2.0 * _g2;
  final y2 = y0 - 1.0 + 2.0 * _g2;
  final n =
      _corner(seed, i, j, x0, y0) +
      _corner(seed, i + i1, j + j1, x1, y1) +
      _corner(seed, i + 1.0, j + 1.0, x2, y2);
  return (70.0 * n).clamp(-1.0, 1.0);
}

// ---------------------------------------------------------------------------------------
// Dot drift and pulse
// ---------------------------------------------------------------------------------------

/// Evaluates dot drift and radius pulse for one `(config, seed, t, half side)`, hoisting
/// everything that does not depend on the dot so a frame of thousands of dots is cheap.
///
/// [sample] gives exactly `dot_offset` and `dot_radius_scale` of the Rust core.
class DotMotion {
  /// Prepares the per-frame constants.
  DotMotion(this.config, this.seed, this.t, double halfSide)
    : _halfSide = halfSide < 1.0 ? 1.0 : halfSide {
    final theta = _unit(seed, 1) * 2.0 * math.pi;
    _dx = math.cos(theta);
    _dy = math.sin(theta);
    if (config.mood == Mood.breathing) {
      _phase0 = _unit(seed, 2) * 2.0 * math.pi;
      _k = 2.0 * math.pi / (breathWavelengthFrac * _halfSide);
      _timePhase = 2.0 * math.pi * t / breathPeriod;
    } else {
      final feature = 0.6 * _halfSide;
      final wind = _halfSide / 7.0;
      _feature = feature;
      _windDx = _dx * wind * t;
      _windDy = _dy * wind * t;
    }
  }

  /// Which mood.
  final MotionConfig config;

  /// Session motion seed (see [seedFromSession]).
  final int seed;

  /// Seconds since the session started.
  final double t;

  final double _halfSide;
  late final double _dx;
  late final double _dy;
  double _phase0 = 0;
  double _k = 0;
  double _timePhase = 0;
  double _feature = 1;
  double _windDx = 0;
  double _windDy = 0;

  /// Writes the offset (pitches, `|offset| <= 0.12`) of a dot resting at `(x, y)` to
  /// `out[0], out[1]` and the radius multiplier (`[0.95, 1.05]`) to `out[2]`.
  void sample(double x, double y, Float64List out) {
    if (config.mood == Mood.breathing) {
      final phase = _k * (x * _dx + y * _dy) - _timePhase + _phase0;
      final along = math.sin(phase);
      final across = 0.5 * math.cos(phase);
      out[0] = _driftAmplitude * (along * _dx - across * _dy);
      out[1] = _driftAmplitude * (along * _dy + across * _dx);
      out[2] = 1.0 + radiusPulse * along.clamp(-1.0, 1.0);
    } else {
      final u = (x - _windDx) / _feature;
      final v = (y - _windDy) / _feature;
      final nx = _simplex2(seed ^ 0x11, u, v);
      final ny = _simplex2(seed ^ 0x22, u + 31.7, v - 17.3);
      final n = math.max(math.sqrt(nx * nx + ny * ny), 1.0);
      out[0] = _driftAmplitude * nx / n;
      out[1] = _driftAmplitude * ny / n;
      final s = _simplex2(seed ^ 0x33, u, v);
      out[2] = 1.0 + radiusPulse * s.clamp(-1.0, 1.0);
    }
  }
}

/// Offset of a dot from its lattice point `(x, y)`, in pitches (`|offset| <= 0.12`).
///
/// [halfSide] is the frame half-side `R` ([FrameSpec.frameHalfSide]). Breathing is a
/// travelling sine of period 2.5 s and wavelength `0.6 R`; the flow field is seeded
/// simplex noise advected slowly across the frame.
({double dx, double dy}) dotOffset(
  MotionConfig cfg,
  int seed,
  double t,
  double x,
  double y,
  double halfSide,
) {
  final out = Float64List(3);
  DotMotion(cfg, seed, t, halfSide).sample(x, y, out);
  return (dx: out[0], dy: out[1]);
}

/// Multiplier for a dot's radius at `(x, y)`, in `[0.95, 1.05]`.
double dotRadiusScale(
  MotionConfig cfg,
  int seed,
  double t,
  double x,
  double y,
  double halfSide,
) {
  final out = Float64List(3);
  DotMotion(cfg, seed, t, halfSide).sample(x, y, out);
  return out[2];
}

/// Scale of hero anchor [anchorIndex] (0..3; index 3 is the centre mark) at time [t].
/// `1.0` unless the heartbeat is on; never above `1.10`.
double anchorScale(MotionConfig cfg, double t, int anchorIndex) {
  if (!cfg.anchorHeartbeat || anchorIndex >= 3) return 1.0;
  final cycle = t / heartbeatPeriod - anchorIndex / 3.0;
  final beat = 0.5 - 0.5 * math.cos(2.0 * math.pi * cycle);
  return 1.0 + heartbeatSwell * beat * beat;
}

// ---------------------------------------------------------------------------------------
// Event-driven curves
// ---------------------------------------------------------------------------------------

/// How much a dot at `(x, y)` lightens (`0..0.15`, as a fraction of the way to white)
/// [tSinceSegmentComplete] seconds after a segment completed.
double rippleLighten(
  double tSinceSegmentComplete,
  double x,
  double y,
  double halfSide,
) {
  final u = tSinceSegmentComplete / rippleDuration;
  if (!(u >= 0.0 && u < 1.0)) return 0.0;
  final ring = u * halfSide * 1.05;
  final width = 0.18 * math.max(halfSide, 1.0);
  final xx = (math.sqrt(x * x + y * y) - ring) / width;
  if (xx.abs() >= 1.0) return 0.0;
  final bump = 0.5 + 0.5 * math.cos(math.pi * xx);
  final envelope = math.sin(math.pi * u);
  return (rippleStrength * bump * envelope).clamp(0.0, rippleStrength);
}

/// Opacity of a dot at `(x, y)`, [tSinceStart] seconds into the session: dots fade in
/// from the centre outwards and are fully opaque from 600 ms on.
double revealOpacity(double tSinceStart, double x, double y, double halfSide) {
  if (tSinceStart >= revealDuration) return 1.0;
  final s = tSinceStart / revealDuration;
  final d = (math.sqrt(x * x + y * y) / math.max(halfSide, 1.0)).clamp(
    0.0,
    1.0,
  );
  return _smoothstep((s - 0.6 * d) / 0.4);
}

/// Opacity of the *new* data frame [tSinceSwitch] seconds after the switch: 0 at the
/// switch, 1 from 40 ms on.
double crossfadeAlpha(double tSinceSwitch) =>
    (tSinceSwitch / crossfadeDuration).clamp(0.0, 1.0);

// ---------------------------------------------------------------------------------------
// Particles
// ---------------------------------------------------------------------------------------

/// Pose `(angle, radius)` of particle [k] around the frame centre, in radians and pitches.
///
/// Within [particleGlide] of a switch the particle glides from [fromAngle] to
/// [toAngle]; otherwise it wobbles by less than 0.8 degrees around [toAngle] and drifts
/// radially inside [band], always keeping the whole speck inside it.
({double angle, double radius}) particlePose(
  int seed,
  double t,
  int k,
  double fromAngle,
  double toAngle,
  double tSinceSwitch,
  ParticleBand band,
) {
  final ks = k + 1;
  final f1 = 0.31 + 0.17 * _unit(seed, 100 + ks);
  final f2 = 0.53 + 0.23 * _unit(seed, 200 + ks);
  final p1 = _unit(seed, 300 + ks) * 2.0 * math.pi;
  final p2 = _unit(seed, 400 + ks) * 2.0 * math.pi;
  final wobble =
      particleWobble *
      (0.6 * math.sin(2.0 * math.pi * f1 * t + p1) +
          0.38 * math.sin(2.0 * math.pi * f2 * t + p2));

  final glide = _smoothstep(tSinceSwitch / particleGlide);
  final centre = fromAngle + (toAngle - fromAngle) * glide;
  final angle = centre + wobble;

  final half = math.max(
    0.5 * (band.outer - band.inner) - CodeDims.particleR,
    0.0,
  );
  final g1 = 0.11 + 0.09 * _unit(seed, 500 + ks);
  final g2 = 0.27 + 0.13 * _unit(seed, 600 + ks);
  final r =
      0.65 *
          math.sin(
            2.0 * math.pi * g1 * t + _unit(seed, 700 + ks) * 2.0 * math.pi,
          ) +
      0.35 *
          math.sin(
            2.0 * math.pi * g2 * t + _unit(seed, 800 + ks) * 2.0 * math.pi,
          );
  return (angle: angle, radius: band.mid + half * r.clamp(-1.0, 1.0));
}
