import 'dart:typed_data';

import 'exception.dart';
import 'frame_spec.dart';
import 'init.dart';
import 'rust/api/dto.dart' as dto;
import 'rust/api/encoder.dart' as rust;
import 'rust/api/error.dart' show FfiError;
import 'rust/api/liveness.dart' as rust_liveness;

/// How a [Shape] is chosen by the encoder.
enum ShapeMode {
  /// Use exactly this shape; fail if it is unknown or does not fit.
  require,

  /// Try this shape first and fall back to the disc.
  prefer,

  /// Try several shapes in order; the disc is the last resort.
  auto,
}

/// Which mask the dot field uses. Ids match `particlecode_core`'s shape table:
/// disc 0, rounded square 1, letters A to Z 2 to 27, heart 28, hexagon 29, ring 30.
class Shape {
  /// Exactly the shape with this table [id].
  const Shape(this.id) : mode = ShapeMode.require, candidates = const [];

  const Shape._(this.id, this.mode, this.candidates);

  /// Tries [shapes] in order, with the disc as the last resort.
  factory Shape.auto(List<Shape> shapes) => Shape._(
    shapes.isEmpty ? 0 : shapes.first.id,
    ShapeMode.auto,
    List.unmodifiable(shapes.map((s) => s.id)),
  );

  /// The letter shape `A` to `Z` (case-insensitive).
  factory Shape.letter(String letter) {
    if (letter.length != 1) {
      throw ArgumentError.value(
        letter,
        'letter',
        'must be a single A-Z letter',
      );
    }
    final c = letter.toUpperCase().codeUnitAt(0);
    if (c < 0x41 || c > 0x5A) {
      throw ArgumentError.value(
        letter,
        'letter',
        'must be a single A-Z letter',
      );
    }
    return Shape(2 + c - 0x41);
  }

  /// The disc (default).
  static const Shape disc = Shape(0);

  /// A rounded square.
  static const Shape roundedSquare = Shape(1);

  /// A heart.
  static const Shape heart = Shape(28);

  /// A hexagon.
  static const Shape hexagon = Shape(29);

  /// A ring.
  static const Shape ring = Shape(30);

  /// Shape table id (for [ShapeMode.auto], the first candidate).
  final int id;

  /// How the encoder uses the shape.
  final ShapeMode mode;

  /// Candidate ids when [mode] is [ShapeMode.auto].
  final List<int> candidates;

  /// This shape, but falling back to the disc if it does not fit.
  Shape get preferred => Shape._(id, ShapeMode.prefer, const []);

  /// The JSON form of the shape choice (`{"kind":"require","id":0}` and so on).
  Map<String, Object?> toJson() => switch (mode) {
    ShapeMode.require => {'kind': 'require', 'id': id},
    ShapeMode.prefer => {'kind': 'prefer', 'id': id},
    ShapeMode.auto => {'kind': 'auto', 'ids': candidates},
  };

  @override
  bool operator ==(Object other) =>
      other is Shape &&
      other.id == id &&
      other.mode == mode &&
      _listEquals(other.candidates, candidates);

  @override
  int get hashCode => Object.hash(id, mode, Object.hashAll(candidates));

  @override
  String toString() => 'Shape($id, ${mode.name})';
}

bool _listEquals(List<int> a, List<int> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

/// Physical display facts used to pick a profile when `profile` is `null`.
class DisplayInfo {
  /// Creates display info. The defaults describe a typical phone.
  const DisplayInfo({
    this.widthPx = 1080,
    this.heightPx = 2400,
    this.widthMm = 68.0,
    this.heightMm = 151.0,
  });

  /// Width of the area available to the code, in pixels.
  final int widthPx;

  /// Height of the area available to the code, in pixels.
  final int heightPx;

  /// Physical width of that area in millimetres.
  final double widthMm;

  /// Physical height of that area in millimetres.
  final double heightMm;

  /// JSON form of the display facts.
  Map<String, Object?> toJson() => {
    'width_px': widthPx,
    'height_px': heightPx,
    'width_mm': widthMm,
    'height_mm': heightMm,
  };
}

/// Options for [ParticleEncoder.create].
class EncoderOptions {
  /// Creates options. Everything is optional.
  const EncoderOptions({
    this.profile = CodeProfile.m,
    this.shape = Shape.disc,
    this.segmentKbLog2 = 3,
    this.fps = 10,
    this.isStatic = false,
    this.repairPerPass = 2,
    this.minPitchMm = 1.6,
    this.sessionId,
    this.key,
    this.clockOriginUnix,
    this.display,
  });

  /// Lattice density, or `null` to pick one from [display] and [minPitchMm].
  final CodeProfile? profile;

  /// Mask selection.
  final Shape shape;

  /// Segment size is `1024 << segmentKbLog2` bytes (0 to 15).
  final int segmentKbLog2;

  /// Data frames per second; also the rate of the liveness clock.
  final int fps;

  /// Emit a single self-contained frame with the stronger ECC (print, poster).
  final bool isStatic;

  /// Repair packets appended to each segment per pass.
  final int repairPerPass;

  /// Smallest acceptable dot pitch in millimetres for automatic profile selection.
  final double minPitchMm;

  /// Fixed 16-bit session id, or `null` for a random one.
  final int? sessionId;

  /// Fixed 32-byte session key, or `null` for a random one.
  final List<int>? key;

  /// Unix second at which frame 0 is shown, or `null` for the current time. Setting
  /// [sessionId], [key] and this makes frames deterministic.
  final int? clockOriginUnix;

  /// The display the code will be shown on.
  final DisplayInfo? display;

  /// The options as a JSON map (snake_case keys of the Rust `EncoderConfig`).
  Map<String, Object?> toJson() => {
    'segment_kb_log2': segmentKbLog2,
    'profile': profile?.letter,
    'fps': fps,
    'is_static': isStatic,
    'shape': shape.toJson(),
    'repair_per_pass': repairPerPass,
    'min_pitch_mm': minPitchMm,
    if (display != null) 'display': display!.toJson(),
    if (sessionId != null) 'session_id': sessionId,
    if (key != null) 'key': key,
    if (clockOriginUnix != null) 'clock_origin_unix': clockOriginUnix,
  };
}

/// Facts about an encoder session.
class EncoderInfo {
  /// Creates info.
  const EncoderInfo({
    required this.profile,
    required this.shapeId,
    required this.dotCount,
    required this.blockSize,
    required this.segmentCount,
    required this.framesPerPass,
    required this.sessionId,
    required this.fps,
    required this.frameHalfSide,
    required this.quietZone,
  });

  /// Parses the JSON form of the Rust `EncoderInfo`.
  factory EncoderInfo.fromJson(Map<String, Object?> json) => EncoderInfo(
    profile: CodeProfile.fromLetter(json['profile']! as String),
    shapeId: (json['shape_id']! as num).toInt(),
    dotCount: (json['dot_count']! as num).toInt(),
    blockSize: (json['block_size']! as num).toInt(),
    segmentCount: (json['segment_count']! as num).toInt(),
    framesPerPass: (json['frames_per_pass']! as num).toInt(),
    sessionId: (json['session_id']! as num).toInt(),
    fps: (json['fps']! as num).toInt(),
    frameHalfSide: (json['frame_half_side']! as num).toDouble(),
    quietZone: (json['quiet_zone']! as num).toDouble(),
  );

  /// Profile of every frame.
  final CodeProfile profile;

  /// Shape id of every frame.
  final int shapeId;

  /// Payload dots per frame.
  final int dotCount;

  /// Payload bytes carried per frame.
  final int blockSize;

  /// Number of segments.
  final int segmentCount;

  /// Frames in one pass over all segments.
  final int framesPerPass;

  /// Session id carried by every header.
  final int sessionId;

  /// Data frames per second.
  final int fps;

  /// Frame half-side `R` in pitches.
  final double frameHalfSide;

  /// Quiet zone around the frame in pitches.
  final double quietZone;
}

/// An encoder session: turns a payload (typically COSE/CBOR) into a stream of
/// [FrameSpec]s for [ParticleCodeView] to paint.
///
/// Backed by a Rust object behind flutter_rust_bridge; call `ParticleCode.init()` once
/// before creating one. Call [dispose] when done; the bridge frees the object if you
/// forget, but only when the garbage collector gets around to it.
class ParticleEncoder {
  ParticleEncoder._(this._inner, this.info, this.key);

  /// Creates an encoder for [payload].
  ///
  /// Throws a [StateError] if `ParticleCode.init()` has not completed, and a
  /// [ParticleCodeException] with the native error text if the options are invalid or
  /// the payload does not fit.
  factory ParticleEncoder.create(
    Uint8List payload, {
    EncoderOptions options = const EncoderOptions(),
  }) {
    ParticleCode.checkInitialized();
    final config = _toConfig(options);
    final rust.ParticleEncoder inner;
    try {
      inner = rust.ParticleEncoder(payload: payload, config: config);
    } on FfiError catch (e) {
      throw ParticleCodeException(e.message());
    }
    try {
      return ParticleEncoder._(
        inner,
        _toInfo(inner.info()),
        Uint8List.fromList(inner.key()),
      );
    } on Object {
      inner.dispose();
      rethrow;
    }
  }

  final rust.ParticleEncoder _inner;
  bool _disposed = false;
  int _inFlight = 0;

  /// Facts about the session: profile, shape, dot count, segments, frame geometry.
  final EncoderInfo info;

  /// The 32-byte session key (share it out of band with whoever verifies liveness).
  final Uint8List key;

  /// Session id carried by every header.
  int get sessionId => info.sessionId;

  /// Frames in one pass over all segments. Larger indices continue with fresh repair
  /// symbols, so the stream never repeats.
  int get framesPerPass {
    _checkAlive();
    return info.framesPerPass;
  }

  /// Data frames per second the session was configured with.
  int get fps => info.fps;

  /// Whether [dispose] has been called.
  bool get isDisposed => _disposed;

  /// Frame [i], built on the calling isolate. With [unixSeconds] the particles show the
  /// liveness value of that wall clock second (use the real clock when displaying
  /// live); without it the clock is derived from the frame index and the session's
  /// clock origin (deterministic).
  ///
  /// Costs a few milliseconds at profile M or L; to keep a UI thread free use
  /// [frameAsync].
  FrameSpec frame(int i, {int? unixSeconds}) {
    _checkAlive();
    _checkArgs(i, unixSeconds);
    final dto = unixSeconds == null
        ? _inner.frame(i: i)
        : _inner.frameAt(i: i, unixSeconds: unixSeconds);
    return FrameSpec.fromDto(dto);
  }

  /// Like [frame], but the frame is built on the bridge's Rust thread pool and the
  /// returned future completes with it; the calling isolate only converts the typed
  /// arrays. Any number of calls may be in flight; they are independent. Disposing the
  /// encoder while calls are in flight is safe: the native object is released when the
  /// last one completes.
  Future<FrameSpec> frameAsync(int i, {int? unixSeconds}) async {
    _checkAlive();
    _checkArgs(i, unixSeconds);
    _inFlight++;
    try {
      final dto = await _inner.frameAsync(i: i, unixSeconds: unixSeconds);
      return FrameSpec.fromDto(dto);
    } finally {
      _inFlight--;
      if (_disposed && _inFlight == 0) _release();
    }
  }

  /// The raw JSON text of frame [i] (see [frame]).
  String frameJson(int i, {int? unixSeconds}) {
    _checkAlive();
    _checkArgs(i, unixSeconds);
    return unixSeconds == null
        ? _inner.frameJson(i: i)
        : _inner.frameJsonAt(i: i, unixSeconds: unixSeconds);
  }

  /// Releases the native encoder. Idempotent; the encoder is unusable afterwards.
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    if (_inFlight == 0) _release();
  }

  void _release() {
    if (!_inner.isDisposed) _inner.dispose();
  }

  void _checkAlive() {
    if (_disposed) {
      throw StateError('ParticleEncoder used after dispose()');
    }
  }

  static void _checkArgs(int i, int? unixSeconds) {
    RangeError.checkValueInInterval(i, 0, 0xFFFFFFFF, 'i');
    if (unixSeconds != null && unixSeconds < 0) {
      throw RangeError.value(
        unixSeconds,
        'unixSeconds',
        'must not be negative',
      );
    }
  }
}

dto.EncoderConfig _toConfig(EncoderOptions o) {
  final display = o.display ?? const DisplayInfo();
  for (final (name, v) in [
    ('minPitchMm', o.minPitchMm),
    ('display.widthMm', display.widthMm),
    ('display.heightMm', display.heightMm),
  ]) {
    if (!v.isFinite) {
      throw ParticleCodeException('invalid encoder options: $name is $v');
    }
  }
  return dto.EncoderConfig(
    segmentKbLog2: o.segmentKbLog2,
    profile: o.profile == null ? null : dto.ProfileDto.values[o.profile!.index],
    fps: o.fps,
    isStatic: o.isStatic,
    shape: switch (o.shape.mode) {
      ShapeMode.require => dto.ShapeChoiceDto.require(id: o.shape.id),
      ShapeMode.prefer => dto.ShapeChoiceDto.prefer(id: o.shape.id),
      ShapeMode.auto => dto.ShapeChoiceDto.auto(
        ids: Uint8List.fromList(o.shape.candidates),
      ),
    },
    repairPerPass: o.repairPerPass,
    minPitchMm: o.minPitchMm,
    display: dto.DisplayInfoDto(
      widthPx: display.widthPx,
      heightPx: display.heightPx,
      widthMm: display.widthMm,
      heightMm: display.heightMm,
    ),
    sessionId: o.sessionId,
    key: o.key == null ? null : Uint8List.fromList(o.key!),
    clockOriginUnix: o.clockOriginUnix,
  );
}

EncoderInfo _toInfo(dto.EncoderInfo i) => EncoderInfo(
  profile: CodeProfile.values[i.profile.index],
  shapeId: i.shapeId,
  dotCount: i.dotCount,
  blockSize: i.blockSize,
  segmentCount: i.segmentCount,
  framesPerPass: i.framesPerPass,
  sessionId: i.sessionId,
  fps: i.fps,
  frameHalfSide: i.frameHalfSide,
  quietZone: i.quietZone,
);

/// Checks a liveness [rolling] value from a scanned code against [key].
///
/// Returns `true` if [rolling] is the liveness value of some second within [skew]
/// seconds of [nowUnix]. [key] must be 32 bytes. Needs `ParticleCode.init()`.
bool livenessVerify(
  List<int> key,
  int rolling, {
  required int nowUnix,
  int skew = 2,
}) {
  if (key.length != 32) {
    throw ArgumentError.value(key.length, 'key.length', 'must be 32');
  }
  ParticleCode.checkInitialized();
  return rust_liveness.livenessVerify(
    key: key,
    rolling: rolling,
    nowUnix: nowUnix,
    skew: skew,
  );
}
