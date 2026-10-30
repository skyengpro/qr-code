import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';

import 'exception.dart';
import 'ffi/bindings.dart';
import 'frame_json.dart';
import 'frame_spec.dart';

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

  /// The JSON form of the C ABI config (`{"kind":"require","id":0}` and so on).
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

  /// JSON form of the C ABI config.
  Map<String, Object?> toJson() => {
    'width_px': widthPx,
    'height_px': heightPx,
    'width_mm': widthMm,
    'height_mm': heightMm,
  };
}

/// Options for [ParticleEncoder.create]. Mirrors the C ABI's JSON config.
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

  /// The JSON config of `pc_encoder_new`.
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

/// Facts about an encoder session (`pc_encoder_info_json`).
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

  /// Parses the JSON of `pc_encoder_info_json`.
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

/// Everything a background isolate needs to rebuild an encoder that produces the same
/// frames as the one it came from. Plain data, so it can be sent through a `SendPort`.
class EncoderWorkerSpec {
  const EncoderWorkerSpec._(this.payload, this.configJson);

  /// The payload bytes.
  final Uint8List payload;

  /// The JSON config for `pc_encoder_new` with the session identity pinned.
  final String configJson;

  /// Rebuilds a spec from [toMessage]'s output.
  factory EncoderWorkerSpec.fromMessage(List<Object?> m) =>
      EncoderWorkerSpec._(m[0]! as Uint8List, m[1]! as String);

  /// A form suitable for `SendPort.send`.
  List<Object?> toMessage() => [payload, configJson];
}

/// An encoder session: turns a payload (typically COSE/CBOR) into a stream of
/// [FrameSpec]s for [ParticleCodeView] to paint.
///
/// Backed by a native handle. Call [dispose] when done; a `NativeFinalizer` frees the
/// handle if you forget, but only when the garbage collector gets around to it.
class ParticleEncoder implements Finalizable {
  ParticleEncoder._(this._bindings, this._handle, this.info, this.key) {
    _finalizerFor(_bindings).attach(this, _handle.cast<Void>(), detach: this);
  }

  /// Creates an encoder for [payload].
  ///
  /// Throws [ParticleCodeException] with the native error text if the options are
  /// invalid, the payload does not fit, or the native library cannot be loaded.
  factory ParticleEncoder.create(
    Uint8List payload, {
    EncoderOptions options = const EncoderOptions(),
  }) {
    final enc = ParticleEncoder.createWith(
      ParticleBindings.instance,
      payload,
      options: options,
    );
    enc._workerSpec = EncoderWorkerSpec._(
      Uint8List.fromList(payload),
      enc._identityConfigJson(options),
    );
    return enc;
  }

  /// Like [ParticleEncoder.create] with explicit [bindings] (tests). Such an encoder
  /// cannot be mirrored in a background isolate ([workerSpec] is `null`).
  factory ParticleEncoder.createWith(
    ParticleBindings bindings,
    Uint8List payload, {
    EncoderOptions options = const EncoderOptions(),
  }) {
    final String configJson;
    try {
      configJson = jsonEncode(options.toJson());
    } on JsonUnsupportedObjectError catch (e) {
      throw ParticleCodeException('invalid encoder options: $e');
    }
    return _createRaw(bindings, payload, configJson);
  }

  /// Creates an encoder from a raw JSON config using the process-wide bindings. This is
  /// what a background isolate does with an [EncoderWorkerSpec].
  factory ParticleEncoder.fromWorkerSpec(EncoderWorkerSpec spec) =>
      _createRaw(ParticleBindings.instance, spec.payload, spec.configJson);

  static ParticleEncoder _createRaw(
    ParticleBindings bindings,
    Uint8List payload,
    String configJson,
  ) {
    final configBytes = utf8.encode(configJson);

    final payloadPtr = calloc<Uint8>(payload.isEmpty ? 1 : payload.length);
    final configPtr = calloc<Uint8>(configBytes.length + 1);
    final errPtr = calloc<Pointer<Utf8>>();
    try {
      payloadPtr.asTypedList(payload.length).setAll(0, payload);
      configPtr.asTypedList(configBytes.length).setAll(0, configBytes);
      final handle = bindings.encoderNew(
        payloadPtr,
        payload.length,
        configPtr,
        configBytes.length,
        errPtr,
      );
      final err = bindings.takeString(errPtr.value);
      if (handle == nullptr) {
        throw ParticleCodeException(err ?? 'pc_encoder_new failed');
      }
      try {
        final info = EncoderInfo.fromJson(
          _decodeObject(
            bindings.takeString(bindings.encoderInfoJson(handle)),
            'encoder info',
          ),
        );
        final keyBytes = _readKey(bindings, handle);
        return ParticleEncoder._(bindings, handle, info, keyBytes);
      } on Object {
        bindings.encoderFree(handle);
        rethrow;
      }
    } finally {
      calloc.free(payloadPtr);
      calloc.free(configPtr);
      calloc.free(errPtr);
    }
  }

  /// A config that makes another encoder emit exactly the same frames as this one when
  /// asked for `frame(i, unixSeconds: s)`: the resolved profile and shape, and the
  /// identity (session id, key) pinned. The liveness clock origin only matters for
  /// frames requested without `unixSeconds`.
  String _identityConfigJson(EncoderOptions options) {
    final cfg = options.toJson()
      ..['profile'] = info.profile.letter
      ..['shape'] = {'kind': 'require', 'id': info.shapeId}
      ..['session_id'] = info.sessionId
      ..['key'] = key.toList()
      ..['clock_origin_unix'] = options.clockOriginUnix ?? 0
      ..remove('display');
    return jsonEncode(cfg);
  }

  EncoderWorkerSpec? _workerSpec;

  /// What a background isolate needs to build an identical encoder, or `null` for an
  /// encoder made with [ParticleEncoder.createWith].
  EncoderWorkerSpec? get workerSpec => _workerSpec;

  int _frameCalls = 0;

  /// How many frames this instance has built (on the isolate that owns it). For tests.
  @visibleForTesting
  int get debugFrameCalls => _frameCalls;

  // One NativeFinalizer per distinct `pc_encoder_free` address, kept alive for the
  // life of the isolate.
  static final Map<int, NativeFinalizer> _finalizers = {};

  static NativeFinalizer _finalizerFor(ParticleBindings bindings) =>
      _finalizers.putIfAbsent(
        bindings.encoderFreeAddress.address,
        () => NativeFinalizer(bindings.encoderFreeAddress),
      );

  final ParticleBindings _bindings;
  Pointer<PcEncoder> _handle;
  bool _disposed = false;

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
    return _bindings.encoderFramesPerPass(_handle);
  }

  /// Data frames per second the session was configured with.
  int get fps => info.fps;

  /// Whether [dispose] has been called.
  bool get isDisposed => _disposed;

  /// Frame [i]. With [unixSeconds] the particles show the liveness value of that wall
  /// clock second (use the real clock when displaying live); without it the clock is
  /// derived from the frame index and the session's clock origin (deterministic).
  FrameSpec frame(int i, {int? unixSeconds}) {
    _checkAlive();
    RangeError.checkValueInInterval(i, 0, 0xFFFFFFFF, 'i');
    _frameCalls++;
    final ptr = unixSeconds == null
        ? _bindings.encoderFrameJson(_handle, i)
        : _bindings.encoderFrameJsonAt(_handle, i, unixSeconds);
    if (ptr == nullptr) {
      throw ParticleCodeException('pc_encoder_frame_json returned NULL for $i');
    }
    try {
      // Parse straight from the library's UTF-8 buffer: no intermediate String.
      final bytes = ptr.cast<Uint8>().asTypedList(ptr.length);
      return parseFrameJsonBytes(bytes, dotCountHint: info.dotCount);
    } on FormatException catch (e) {
      throw ParticleCodeException('malformed frame $i JSON: ${e.message}');
    } finally {
      _bindings.stringFree(ptr);
    }
  }

  /// The raw JSON text of frame [i] (see [frame]).
  String frameJson(int i, {int? unixSeconds}) {
    _checkAlive();
    final ptr = unixSeconds == null
        ? _bindings.encoderFrameJson(_handle, i)
        : _bindings.encoderFrameJsonAt(_handle, i, unixSeconds);
    return _bindings.takeString(ptr) ??
        (throw const ParticleCodeException(
          'pc_encoder_frame_json returned NULL',
        ));
  }

  /// Releases the native encoder. Idempotent; the encoder is unusable afterwards.
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _finalizerFor(_bindings).detach(this);
    _bindings.encoderFree(_handle);
    _handle = nullptr;
  }

  void _checkAlive() {
    if (_disposed) {
      throw StateError('ParticleEncoder used after dispose()');
    }
  }
}

Uint8List _readKey(ParticleBindings bindings, Pointer<PcEncoder> handle) {
  final out = calloc<Uint8>(32);
  try {
    final rc = bindings.encoderKeyRaw(handle, out);
    if (rc != 0) {
      throw ParticleCodeException('pc_encoder_key failed ($rc)');
    }
    return Uint8List.fromList(out.asTypedList(32));
  } finally {
    calloc.free(out);
  }
}

Map<String, Object?> _decodeObject(String? text, String what) {
  if (text == null) {
    throw ParticleCodeException('native library returned no $what');
  }
  try {
    return jsonDecode(text) as Map<String, Object?>;
  } on FormatException catch (e) {
    throw ParticleCodeException('malformed $what JSON: ${e.message}');
  }
}

/// Checks a liveness [rolling] value from a scanned code against [key].
///
/// Returns `true` if [rolling] is the liveness value of some second within [skew]
/// seconds of [nowUnix] (`pc_liveness_verify`). [key] must be 32 bytes.
bool livenessVerify(
  List<int> key,
  int rolling, {
  required int nowUnix,
  int skew = 2,
}) {
  if (key.length != 32) {
    throw ArgumentError.value(key.length, 'key.length', 'must be 32');
  }
  final bindings = ParticleBindings.instance;
  final keyPtr = calloc<Uint8>(32);
  try {
    keyPtr.asTypedList(32).setAll(0, key);
    final rc = bindings.livenessVerifyRaw(keyPtr, 32, rolling, nowUnix, skew);
    if (rc < 0) {
      throw ParticleCodeException('pc_liveness_verify failed ($rc)');
    }
    return rc == 1;
  } finally {
    calloc.free(keyPtr);
  }
}
