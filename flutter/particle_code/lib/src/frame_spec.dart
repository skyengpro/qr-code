import 'dart:collection';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

import 'palette.dart';

/// Lattice density announced in the header.
enum CodeProfile {
  /// 48 dots across the field. Fallback for weak cameras.
  s('S', 48),

  /// 72 dots across the field. The default.
  m('M', 72),

  /// 96 dots across the field. 1080p-or-better cameras held close.
  l('L', 96);

  const CodeProfile(this.letter, this.dotsAcross);

  /// The profile's JSON/ABI name: `"S"`, `"M"` or `"L"`.
  final String letter;

  /// Dots across the field.
  final int dotsAcross;

  /// Profile for a JSON letter.
  static CodeProfile fromLetter(String letter) {
    for (final p in values) {
      if (p.letter == letter) return p;
    }
    throw FormatException('unknown profile "$letter"');
  }

  /// Profile for the integer code used by the scanner protocol (0 S, 1 M, 2 L), or
  /// `null` when unknown (-1 or anything else).
  static CodeProfile? fromCode(int code) =>
      code >= 0 && code < values.length ? values[code] : null;
}

/// Dot geometry constants of the format (`particlecode_core::layout::dims`), in pitches.
abstract final class CodeDims {
  /// Small data dot radius.
  static const double dotRSmall = 0.36;

  /// Large data dot radius.
  static const double dotRLarge = 0.48;

  /// Calibration swatch dot radius.
  static const double swatchDotR = 0.42;

  /// Particle speck radius.
  static const double particleR = 0.42;

  /// Minimum quiet zone around the frame.
  static const double quietZone = 1.5;
}

/// What a calibration swatch shows.
enum SwatchKind {
  /// One of the four data hues (see [SwatchColor.hue]).
  hue,

  /// The white reference.
  white,

  /// The black reference.
  black,
}

/// Color of a calibration swatch: a hue index, white or black.
class SwatchColor {
  /// A data hue `0..4`.
  const SwatchColor.hue(this.hue) : kind = SwatchKind.hue;

  /// The white reference.
  const SwatchColor.white() : kind = SwatchKind.white, hue = 0;

  /// The black reference.
  const SwatchColor.black() : kind = SwatchKind.black, hue = 0;

  /// Parses serde's form: `{"Hue": 2}`, `"White"` or `"Black"`.
  factory SwatchColor.fromJson(Object? json) {
    if (json == 'White') return const SwatchColor.white();
    if (json == 'Black') return const SwatchColor.black();
    if (json is Map && json['Hue'] is int) {
      return SwatchColor.hue((json['Hue'] as int) & 3);
    }
    throw FormatException('bad swatch color: $json');
  }

  /// Which kind of swatch this is.
  final SwatchKind kind;

  /// Hue index; only meaningful when [kind] is [SwatchKind.hue].
  final int hue;

  /// The exact sRGB color.
  Color get color => switch (kind) {
    SwatchKind.hue => ParticleCodePalette.hues[hue & 3],
    SwatchKind.white => ParticleCodePalette.white,
    SwatchKind.black => ParticleCodePalette.black,
  };

  @override
  bool operator ==(Object other) =>
      other is SwatchColor && other.kind == kind && other.hue == hue;

  @override
  int get hashCode => Object.hash(kind, hue);

  @override
  String toString() => kind == SwatchKind.hue ? 'Hue($hue)' : kind.name;
}

/// A ring-and-core anchor mark. Draw the ring, then the core, then the pupil.
class AnchorSpec {
  /// Creates an anchor.
  const AnchorSpec({
    required this.at,
    required this.coreR,
    required this.ringIn,
    required this.ringOut,
    required this.pupilR,
  });

  /// Parses the serde form.
  factory AnchorSpec.fromJson(Map<String, Object?> json) => AnchorSpec(
    at: _point(json['at']),
    coreR: _num(json['core_r']),
    ringIn: _num(json['ring_in']),
    ringOut: _num(json['ring_out']),
    pupilR: _num(json['pupil_r']),
  );

  /// Center, pitch units, y down.
  final Offset at;

  /// Solid core radius.
  final double coreR;

  /// Ring inner radius.
  final double ringIn;

  /// Ring outer radius.
  final double ringOut;

  /// Radius of the background-colored pupil in the core (0 = none).
  final double pupilR;
}

/// A calibration swatch.
class Swatch {
  /// Creates a swatch.
  const Swatch({
    required this.at,
    required this.color,
    required this.anchor,
    required this.slot,
  });

  /// Parses the serde form.
  factory Swatch.fromJson(Map<String, Object?> json) => Swatch(
    at: _point(json['at']),
    color: SwatchColor.fromJson(json['color']),
    anchor: (json['anchor']! as num).toInt(),
    slot: (json['slot']! as num).toInt(),
  );

  /// Center, pitch units.
  final Offset at;

  /// Color.
  final SwatchColor color;

  /// Which hero anchor's halo (0..3).
  final int anchor;

  /// Slot on the ring (0..16).
  final int slot;
}

/// A data or header dot.
class FrameDot {
  /// Creates a dot.
  const FrameDot({
    required this.q,
    required this.r,
    required this.x,
    required this.y,
    required this.color,
    required this.size,
  });

  /// Parses the serde form.
  factory FrameDot.fromJson(Map<String, Object?> json) {
    final at = json['at']! as Map<String, Object?>;
    return FrameDot(
      q: (json['q']! as num).toInt(),
      r: (json['r']! as num).toInt(),
      x: _num(at['x']),
      y: _num(at['y']),
      color: (json['color']! as num).toInt(),
      size: (json['size']! as num).toInt(),
    );
  }

  /// Axial column (0 for header dots).
  final int q;

  /// Axial row (0 for header dots).
  final int r;

  /// Rest position, pitch units.
  final double x;

  /// Rest position, pitch units.
  final double y;

  /// Hue index `0..4`.
  final int color;

  /// 0 small, 1 large.
  final int size;

  /// Rest radius in pitches.
  double get radius => size == 0 ? CodeDims.dotRSmall : CodeDims.dotRLarge;

  /// The dot's exact sRGB color.
  Color get rgb => ParticleCodePalette.hues[color & 3];
}

/// An immutable list of dots stored as parallel typed arrays.
///
/// A data frame has thousands of dots; holding them as arrays keeps parsing cheap, lets a
/// frame cross isolates as a few typed-data blocks, and lets the painter read the numbers
/// without allocating an object per dot. It still behaves as a `List<FrameDot>` (dots are
/// created on access).
final class FrameDots extends ListBase<FrameDot> {
  /// Wraps the parallel arrays (all of the same length). Not copied.
  FrameDots.fromArrays({
    required this.q,
    required this.r,
    required this.x,
    required this.y,
    required this.color,
    required this.size,
  }) : assert(
         q.length == r.length &&
             q.length == x.length &&
             q.length == y.length &&
             q.length == color.length &&
             q.length == size.length,
         'dot arrays must have the same length',
       );

  /// Copies [dots] into arrays (returns [dots] itself if it already is a [FrameDots]).
  factory FrameDots.from(List<FrameDot> dots) {
    if (dots is FrameDots) return dots;
    final n = dots.length;
    final q = Int32List(n), r = Int32List(n);
    final x = Float64List(n), y = Float64List(n);
    final color = Uint8List(n), size = Uint8List(n);
    for (var i = 0; i < n; i++) {
      final d = dots[i];
      q[i] = d.q;
      r[i] = d.r;
      x[i] = d.x;
      y[i] = d.y;
      color[i] = d.color;
      size[i] = d.size;
    }
    return FrameDots.fromArrays(
      q: q,
      r: r,
      x: x,
      y: y,
      color: color,
      size: size,
    );
  }

  /// Parses the serde list of dots.
  factory FrameDots.fromJson(List<Object?> json) {
    final n = json.length;
    final q = Int32List(n), r = Int32List(n);
    final x = Float64List(n), y = Float64List(n);
    final color = Uint8List(n), size = Uint8List(n);
    for (var i = 0; i < n; i++) {
      final d = json[i]! as Map<String, Object?>;
      final at = d['at']! as Map<String, Object?>;
      q[i] = (d['q']! as num).toInt();
      r[i] = (d['r']! as num).toInt();
      x[i] = (at['x']! as num).toDouble();
      y[i] = (at['y']! as num).toDouble();
      color[i] = (d['color']! as num).toInt();
      size[i] = (d['size']! as num).toInt();
    }
    return FrameDots.fromArrays(
      q: q,
      r: r,
      x: x,
      y: y,
      color: color,
      size: size,
    );
  }

  /// Axial columns.
  final Int32List q;

  /// Axial rows.
  final Int32List r;

  /// Rest x positions, pitch units.
  final Float64List x;

  /// Rest y positions, pitch units.
  final Float64List y;

  /// Hue indices `0..4`.
  final Uint8List color;

  /// Size classes (0 small, 1 large).
  final Uint8List size;

  @override
  int get length => q.length;

  @override
  set length(int newLength) => throw UnsupportedError('FrameDots is immutable');

  @override
  FrameDot operator [](int index) => FrameDot(
    q: q[index],
    r: r[index],
    x: x[index],
    y: y[index],
    color: color[index],
    size: size[index],
  );

  @override
  void operator []=(int index, FrameDot value) =>
      throw UnsupportedError('FrameDots is immutable');

  /// The arrays as a message for another isolate (see [FrameDots.fromMessage]).
  List<Object?> toMessage() => [q, r, x, y, color, size];

  /// Rebuilds dots from [toMessage]'s output.
  factory FrameDots.fromMessage(List<Object?> m) => FrameDots.fromArrays(
    q: m[0]! as Int32List,
    r: m[1]! as Int32List,
    x: m[2]! as Float64List,
    y: m[3]! as Float64List,
    color: m[4]! as Uint8List,
    size: m[5]! as Uint8List,
  );
}

/// A particle speck.
class ParticleSpec {
  /// Creates a particle.
  const ParticleSpec({required this.sector, required this.angle});

  /// Parses the serde form.
  factory ParticleSpec.fromJson(Map<String, Object?> json) => ParticleSpec(
    sector: (json['sector']! as num).toInt(),
    angle: _num(json['angle']),
  );

  /// Encoded sector `0..16` on this particle's arc.
  final int sector;

  /// Rest angle in radians (sector center).
  final double angle;
}

/// The annulus particles orbit in.
class ParticleBand {
  /// Creates a band.
  const ParticleBand({required this.inner, required this.outer});

  /// Parses the serde form.
  factory ParticleBand.fromJson(Map<String, Object?> json) =>
      ParticleBand(inner: _num(json['inner']), outer: _num(json['outer']));

  /// Inner radius.
  final double inner;

  /// Outer radius.
  final double outer;

  /// Middle radius.
  double get mid => 0.5 * (inner + outer);
}

/// The decoded header a frame carries.
class FrameHeader {
  /// Creates a header.
  const FrameHeader({
    required this.shapeTableVersion,
    required this.profile,
    required this.palette,
    required this.isStatic,
    required this.shapeId,
    required this.sessionId,
    required this.payloadLen,
    required this.segmentKbLog2,
    required this.segmentIndex,
    required this.packetSeed,
  });

  /// Parses the serde form.
  factory FrameHeader.fromJson(Map<String, Object?> json) => FrameHeader(
    shapeTableVersion: (json['shape_table_version']! as num).toInt(),
    profile: CodeProfile.fromLetter(json['profile']! as String),
    palette: json['palette']! as String,
    isStatic: json['is_static']! as bool,
    shapeId: (json['shape_id']! as num).toInt(),
    sessionId: (json['session_id']! as num).toInt(),
    payloadLen: (json['payload_len']! as num).toInt(),
    segmentKbLog2: (json['segment_kb_log2']! as num).toInt(),
    segmentIndex: (json['segment_index']! as num).toInt(),
    packetSeed: (json['packet_seed']! as num).toInt(),
  );

  /// Shape table version.
  final int shapeTableVersion;

  /// Lattice profile.
  final CodeProfile profile;

  /// Palette name (`"Hues4"`).
  final String palette;

  /// `true` if this frame is the whole payload.
  final bool isStatic;

  /// Mask the frame samples.
  final int shapeId;

  /// Random per transfer.
  final int sessionId;

  /// Payload length in bytes.
  final int payloadLen;

  /// Segment size is `2^n` KiB.
  final int segmentKbLog2;

  /// Which segment this packet belongs to.
  final int segmentIndex;

  /// RaptorQ encoding symbol id within the segment.
  final int packetSeed;
}

/// Everything a painter needs to draw one data frame. Pitch units, origin at the frame
/// center, y down. Immutable.
class FrameSpec {
  /// Creates a frame. The dot lists are copied into a [FrameDots] unless they already
  /// are one.
  FrameSpec({
    required this.index,
    required this.header,
    required this.profile,
    required this.frameHalfSide,
    required this.particleBand,
    required this.anchors,
    required this.swatches,
    required List<FrameDot> headerDots,
    required List<FrameDot> dots,
    required this.particles,
  }) : headerDots = FrameDots.from(headerDots),
       dots = FrameDots.from(dots);

  /// Parses the JSON text produced by `pc_encoder_frame_json[_at]`.
  ///
  /// [dots] replaces the payload dots of [json] when given (used by the fast parser).
  factory FrameSpec.fromJson(Map<String, Object?> json, {FrameDots? dots}) {
    List<T> list<T>(String key, T Function(Map<String, Object?>) f) =>
        List.unmodifiable(
          (json[key]! as List<Object?>).map(
            (e) => f(e! as Map<String, Object?>),
          ),
        );
    return FrameSpec(
      index: (json['index']! as num).toInt(),
      header: FrameHeader.fromJson(json['header']! as Map<String, Object?>),
      profile: CodeProfile.fromLetter(json['profile']! as String),
      frameHalfSide: _num(json['frame_half_side']),
      particleBand: ParticleBand.fromJson(
        json['particle_band']! as Map<String, Object?>,
      ),
      anchors: list('anchors', AnchorSpec.fromJson),
      swatches: list('swatches', Swatch.fromJson),
      headerDots: FrameDots.fromJson(json['header_dots']! as List<Object?>),
      dots: dots ?? FrameDots.fromJson(json['dots']! as List<Object?>),
      particles: list('particles', ParticleSpec.fromJson),
    );
  }

  /// Frame index within the session.
  final int index;

  /// The header this frame carries.
  final FrameHeader header;

  /// Profile (also in the header).
  final CodeProfile profile;

  /// Frame half-side `R`. The quiet zone lies outside `[-R, R]^2`.
  final double frameHalfSide;

  /// Particle band.
  final ParticleBand particleBand;

  /// Three hero anchors (top, lower right, lower left) then the center anchor.
  final List<AnchorSpec> anchors;

  /// 48 calibration swatches.
  final List<Swatch> swatches;

  /// 48 header dots.
  final FrameDots headerDots;

  /// Payload dots in symbol order.
  final FrameDots dots;

  /// Six particles.
  final List<ParticleSpec> particles;

  /// Flattens the frame into plain lists and typed arrays that cross an isolate boundary
  /// cheaply. Rebuild with [FrameSpec.fromMessage].
  List<Object?> toMessage() {
    final a = Float64List(anchors.length * 6);
    for (var i = 0; i < anchors.length; i++) {
      final s = anchors[i];
      a.setAll(i * 6, [
        s.at.dx,
        s.at.dy,
        s.coreR,
        s.ringIn,
        s.ringOut,
        s.pupilR,
      ]);
    }
    final sw = Float64List(swatches.length * 2);
    final swi = Uint8List(swatches.length * 4);
    for (var i = 0; i < swatches.length; i++) {
      final s = swatches[i];
      sw[i * 2] = s.at.dx;
      sw[i * 2 + 1] = s.at.dy;
      swi[i * 4] = s.color.kind.index;
      swi[i * 4 + 1] = s.color.hue;
      swi[i * 4 + 2] = s.anchor;
      swi[i * 4 + 3] = s.slot;
    }
    final pa = Float64List(particles.length);
    final ps = Uint8List(particles.length);
    for (var i = 0; i < particles.length; i++) {
      pa[i] = particles[i].angle;
      ps[i] = particles[i].sector;
    }
    final h = header;
    return [
      index,
      [
        h.shapeTableVersion,
        h.profile.index,
        h.palette,
        h.isStatic,
        h.shapeId,
        h.sessionId,
        h.payloadLen,
        h.segmentKbLog2,
        h.segmentIndex,
        h.packetSeed,
      ],
      profile.index,
      frameHalfSide,
      particleBand.inner,
      particleBand.outer,
      a,
      sw,
      swi,
      headerDots.toMessage(),
      dots.toMessage(),
      pa,
      ps,
    ];
  }

  /// Rebuilds a frame from [toMessage]'s output.
  factory FrameSpec.fromMessage(List<Object?> m) {
    final h = m[1]! as List<Object?>;
    final a = m[6]! as Float64List;
    final sw = m[7]! as Float64List;
    final swi = m[8]! as Uint8List;
    final pa = m[11]! as Float64List;
    final ps = m[12]! as Uint8List;
    return FrameSpec(
      index: m[0]! as int,
      header: FrameHeader(
        shapeTableVersion: h[0]! as int,
        profile: CodeProfile.values[h[1]! as int],
        palette: h[2]! as String,
        isStatic: h[3]! as bool,
        shapeId: h[4]! as int,
        sessionId: h[5]! as int,
        payloadLen: h[6]! as int,
        segmentKbLog2: h[7]! as int,
        segmentIndex: h[8]! as int,
        packetSeed: h[9]! as int,
      ),
      profile: CodeProfile.values[m[2]! as int],
      frameHalfSide: m[3]! as double,
      particleBand: ParticleBand(
        inner: m[4]! as double,
        outer: m[5]! as double,
      ),
      anchors: List.unmodifiable([
        for (var i = 0; i < a.length; i += 6)
          AnchorSpec(
            at: Offset(a[i], a[i + 1]),
            coreR: a[i + 2],
            ringIn: a[i + 3],
            ringOut: a[i + 4],
            pupilR: a[i + 5],
          ),
      ]),
      swatches: List.unmodifiable([
        for (var i = 0; i < sw.length ~/ 2; i++)
          Swatch(
            at: Offset(sw[i * 2], sw[i * 2 + 1]),
            color: switch (SwatchKind.values[swi[i * 4]]) {
              SwatchKind.hue => SwatchColor.hue(swi[i * 4 + 1]),
              SwatchKind.white => const SwatchColor.white(),
              SwatchKind.black => const SwatchColor.black(),
            },
            anchor: swi[i * 4 + 2],
            slot: swi[i * 4 + 3],
          ),
      ]),
      headerDots: FrameDots.fromMessage(m[9]! as List<Object?>),
      dots: FrameDots.fromMessage(m[10]! as List<Object?>),
      particles: List.unmodifiable([
        for (var i = 0; i < pa.length; i++)
          ParticleSpec(sector: ps[i], angle: pa[i]),
      ]),
    );
  }

  /// Whether [other] describes exactly the same frame (every field, bit for bit).
  bool sameAs(FrameSpec other) {
    final a = toMessage(), b = other.toMessage();
    return _deepEq(a, b);
  }

  /// Half-side of the visible square for a quiet zone of [margin] pitches
  /// (`R + max(margin, 1.5)`).
  double halfExtent(double margin) =>
      frameHalfSide + math.max(margin, CodeDims.quietZone);
}

double _num(Object? v) => (v! as num).toDouble();

Offset _point(Object? v) {
  final m = v! as Map<String, Object?>;
  return Offset(_num(m['x']), _num(m['y']));
}

bool _deepEq(Object? a, Object? b) {
  if (a is List && b is List) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (!_deepEq(a[i], b[i])) return false;
    }
    return true;
  }
  return a == b;
}
