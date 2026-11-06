import 'dart:typed_data';
import 'dart:ui';

import '../frame_spec.dart';
import 'errors.dart';

/// What the decoder made of the latest camera frame.
enum ScanStatus {
  /// The frame was not used (no code found, unreadable, ...).
  rejected,

  /// The frame was used; progress advanced or was unchanged.
  accepted,

  /// The whole payload arrived.
  done;

  /// Parses the protocol string; anything unknown counts as [rejected].
  static ScanStatus parse(Object? value) => switch (value) {
    'accepted' => accepted,
    'done' => done,
    _ => rejected,
  };
}

/// Advice for the person holding the phone, from the protocol's `hintCode`.
enum ScanHint {
  /// Nothing to say.
  none,

  /// Point the camera at the code.
  pointAtCode,

  /// Move the phone back.
  moveBack,

  /// Hold steady.
  holdSteady,

  /// Too dark or too much glare.
  lighting,

  /// The sender uses a newer format: update the app.
  updateApp,

  /// Anything else; see [ScanFrame.hint].
  other;

  /// Maps the protocol's integer code (unknown codes become [other]).
  static ScanHint fromCode(int code) =>
      code >= 0 && code < values.length ? values[code] : other;
}

/// The particles' liveness reading.
class ScanLiveness {
  /// Creates a reading.
  const ScanLiveness({
    required this.consistent,
    required this.seedLow,
    required this.rolling,
  });

  /// Whether the particles' seed byte matches the header's session id.
  final bool consistent;

  /// Low byte of the session seed as read from the particles.
  final int seedLow;

  /// The rolling value; check it with `livenessVerify`.
  final int rolling;
}

/// An event of the [ParticleScanner] stream.
///
/// [fromMap] parses the plain-map form (kept for tests and tooling); the scanner itself
/// builds the events from decoder results.
sealed class ScanEvent {
  const ScanEvent();

  /// Parses an event map as delivered by the event channel.
  ///
  /// Returns `null` for an unknown `"type"` (forward compatibility). Throws a
  /// [FormatException] if a known event is malformed.
  static ScanEvent? fromMap(Object? raw) {
    if (raw is! Map) {
      throw FormatException('scan event is not a map: ${raw.runtimeType}');
    }
    final map = raw.cast<Object?, Object?>();
    switch (map['type']) {
      case 'frame':
        return ScanFrame.fromMap(map);
      case 'segment':
        return ScanSegment(_bytes(map['bytes'], 'bytes'));
      case 'done':
        return ScanDone(
          _bytes(map['payload'], 'payload'),
          _int(map['sessionId'], 'sessionId'),
        );
      case 'error':
        return ScanError(
          (map['code'] as String?) ?? 'unknown',
          (map['message'] as String?) ?? '',
        );
      default:
        return null;
    }
  }
}

/// Per-camera-frame progress, at most about ten per second and whenever progress changes.
class ScanFrame extends ScanEvent {
  /// Creates a frame event.
  const ScanFrame({
    required this.status,
    required this.locked,
    required this.profile,
    required this.shapeId,
    required this.have,
    required this.need,
    required this.segmentsDone,
    required this.segmentsTotal,
    required this.liveness,
    required this.anchors,
    required this.sharpness,
    required this.hint,
    required this.hintCode,
    required this.rejectReason,
  });

  /// Parses the `"frame"` event map.
  factory ScanFrame.fromMap(Map<Object?, Object?> map) {
    final anchorsRaw = map['anchors'];
    final anchors = <Offset>[];
    if (anchorsRaw is List) {
      for (var i = 0; i + 1 < anchorsRaw.length; i += 2) {
        anchors.add(
          Offset(
            (anchorsRaw[i] as num).toDouble(),
            (anchorsRaw[i + 1] as num).toDouble(),
          ),
        );
      }
    }
    final liveRaw = map['liveness'];
    ScanLiveness? liveness;
    if (liveRaw is Map) {
      liveness = ScanLiveness(
        consistent: liveRaw['consistent'] as bool? ?? false,
        seedLow: _int(liveRaw['seedLow'], 'seedLow'),
        rolling: _int(liveRaw['rolling'], 'rolling'),
      );
    }
    final shapeId = _int(map['shapeId'] ?? -1, 'shapeId');
    return ScanFrame(
      status: ScanStatus.parse(map['status']),
      locked: map['locked'] as bool? ?? false,
      profile: CodeProfile.fromCode(_int(map['profile'] ?? -1, 'profile')),
      shapeId: shapeId < 0 ? null : shapeId,
      have: _int(map['have'] ?? 0, 'have'),
      need: _int(map['need'] ?? 0, 'need'),
      segmentsDone: _int(map['segmentsDone'] ?? 0, 'segmentsDone'),
      segmentsTotal: _int(map['segmentsTotal'] ?? 0, 'segmentsTotal'),
      liveness: liveness,
      anchors: List.unmodifiable(anchors),
      sharpness: (map['sharpness'] as num?)?.toDouble() ?? 0,
      hint: map['hint'] as String?,
      hintCode: ScanHint.fromCode(_int(map['hintCode'] ?? 0, 'hintCode')),
      rejectReason: map['rejectReason'] as String?,
    );
  }

  /// Decoder verdict for this camera frame.
  final ScanStatus status;

  /// Whether the three anchors were found.
  final bool locked;

  /// Profile read from the header, or `null` if not read yet.
  final CodeProfile? profile;

  /// Shape id read from the header, or `null` if not read yet.
  final int? shapeId;

  /// Fountain symbols held (sticky across rejects).
  final int have;

  /// Fountain symbols needed.
  final int need;

  /// Segments decoded and verified.
  final int segmentsDone;

  /// Segments in the transfer.
  final int segmentsTotal;

  /// Particle liveness reading, if the particles were read.
  final ScanLiveness? liveness;

  /// Lock anchors (0, 3 or 4 points), normalized 0..1 in preview coordinates.
  final List<Offset> anchors;

  /// Higher is sharper; 0 if not locked.
  final double sharpness;

  /// User-facing advice text, if any.
  final String? hint;

  /// [hint] as an enum.
  final ScanHint hintCode;

  /// Technical reason for a rejected frame, for logs.
  final String? rejectReason;

  /// Fraction of needed symbols held, `0..1` (0 when [need] is 0).
  double get progress => need <= 0 ? 0 : (have / need).clamp(0.0, 1.0);
}

/// Newly verified, in-order payload bytes (streaming).
class ScanSegment extends ScanEvent {
  /// Creates a segment event.
  const ScanSegment(this.bytes);

  /// The bytes; concatenating all segments gives the payload.
  final Uint8List bytes;
}

/// The whole payload arrived. The scanner stops decoding until `reset`.
class ScanDone extends ScanEvent {
  /// Creates a done event.
  const ScanDone(this.payload, this.sessionId);

  /// The complete payload.
  final Uint8List payload;

  /// Session id of the transfer.
  final int sessionId;
}

/// A camera failure after `start`.
class ScanError extends ScanEvent {
  /// Creates an error event.
  const ScanError(this.code, this.message);

  /// The protocol error code (`camera_error`, ...).
  final String code;

  /// Description for logs.
  final String message;

  /// [code] as an enum.
  ParticleScannerError get kind => ParticleScannerError.fromCode(code);
}

int _int(Object? v, String name) {
  if (v is int) return v;
  if (v is num) return v.toInt();
  throw FormatException('scan event field "$name" is not a number: $v');
}

Uint8List _bytes(Object? v, String name) {
  if (v is Uint8List) return v;
  if (v is List) return Uint8List.fromList(v.cast<int>());
  throw FormatException('scan event field "$name" is not bytes: $v');
}
