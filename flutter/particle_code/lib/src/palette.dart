import 'dart:ui';

/// The exact sRGB colors every Particle Code painter must use
/// (`particlecode_core::profile::colors`). Never color-managed, never filtered.
abstract final class ParticleCodePalette {
  /// The four data hues, indexed by a dot's 2-bit color: blue, green, orange, magenta.
  static const List<Color> hues = [
    Color.fromARGB(255, 20, 110, 245),
    Color.fromARGB(255, 40, 165, 30),
    Color.fromARGB(255, 235, 100, 0),
    Color.fromARGB(255, 215, 40, 175),
  ];

  /// Quiet zone and field background (warm off-white).
  static const Color background = Color.fromARGB(255, 246, 244, 239);

  /// Anchor rings and cores.
  static const Color anchor = Color.fromARGB(255, 24, 28, 52);

  /// White calibration swatch.
  static const Color white = Color.fromARGB(255, 255, 255, 255);

  /// Black calibration swatch.
  static const Color black = Color.fromARGB(255, 12, 12, 16);

  /// Particle specks: neutral grey.
  static const Color particle = Color.fromARGB(255, 120, 122, 128);
}

int _mix(int a, int b, double t) {
  final v = a + (b - a) * t;
  return v.round().clamp(0, 255);
}

/// Straight sRGB interpolation, exact at `t <= 0` and `t >= 1`
/// (port of `particlecode-render`'s `lerp_rgb`).
Color lerpRgb(Color a, Color b, double t) {
  if (t <= 0) return a;
  if (t >= 1) return b;
  return Color.fromARGB(
    255,
    _mix(_r(a), _r(b), t),
    _mix(_g(a), _g(b), t),
    _mix(_b(a), _b(b), t),
  );
}

/// Moves [c] a fraction [amount] of the way to white (the ripple's lighten).
Color lighten(Color c, double amount) =>
    lerpRgb(c, ParticleCodePalette.white, amount);

/// Interpolates through HSV, rotating the hue along the shortest arc
/// (port of `lerp_hue`; the "shimmer" cross-fade).
Color lerpHue(Color a, Color b, double t) {
  if (t <= 0) return a;
  if (t >= 1) return b;
  final (ha, sa, va) = _toHsv(a);
  final (hb, sb, vb) = _toHsv(b);
  var dh = hb - ha;
  if (dh > 180) {
    dh -= 360;
  } else if (dh < -180) {
    dh += 360;
  }
  return _fromHsv(ha + dh * t, sa + (sb - sa) * t, va + (vb - va) * t);
}

int _r(Color c) => (c.toARGB32() >> 16) & 0xFF;
int _g(Color c) => (c.toARGB32() >> 8) & 0xFF;
int _b(Color c) => c.toARGB32() & 0xFF;

double _remEuclid(double a, double b) {
  final r = a % b;
  return r < 0 ? r + b.abs() : r;
}

(double, double, double) _toHsv(Color c) {
  final r = _r(c) / 255.0;
  final g = _g(c) / 255.0;
  final b = _b(c) / 255.0;
  final max = r > g ? (r > b ? r : b) : (g > b ? g : b);
  final min = r < g ? (r < b ? r : b) : (g < b ? g : b);
  final d = max - min;
  final double h;
  if (d < 1e-12) {
    h = 0;
  } else if ((max - r).abs() < 1e-12) {
    h = 60.0 * _remEuclid((g - b) / d, 6.0);
  } else if ((max - g).abs() < 1e-12) {
    h = 60.0 * ((b - r) / d + 2.0);
  } else {
    h = 60.0 * ((r - g) / d + 4.0);
  }
  return (h, max <= 0 ? 0.0 : d / max, max);
}

Color _fromHsv(double h, double s, double v) {
  final c = v * s;
  final hp = _remEuclid(h, 360.0) / 60.0;
  final x = c * (1.0 - (_remEuclid(hp, 2.0) - 1.0).abs());
  final double r, g, b;
  switch (hp.floor()) {
    case 0:
      (r, g, b) = (c, x, 0.0);
    case 1:
      (r, g, b) = (x, c, 0.0);
    case 2:
      (r, g, b) = (0.0, c, x);
    case 3:
      (r, g, b) = (0.0, x, c);
    case 4:
      (r, g, b) = (x, 0.0, c);
    default:
      (r, g, b) = (c, 0.0, x);
  }
  final m = v - c;
  int q(double u) => ((u + m) * 255.0).round().clamp(0, 255);
  return Color.fromARGB(255, q(r), q(g), q(b));
}
