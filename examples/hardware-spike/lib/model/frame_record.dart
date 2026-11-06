import 'package:particle_code/particle_code.dart';

double? _ms(Duration? d) => d == null ? null : d.inMicroseconds / 1000.0;

/// One scanner frame event as recorded for a trial (all durations in milliseconds).
class FrameRecord {
  /// Creates a record.
  const FrameRecord({
    required this.index,
    required this.nativeFrameId,
    required this.disposition,
    required this.sinceStartMs,
    this.captureGapMs,
    this.receiptLatencyMs,
    this.skippedBefore = 0,
    this.heldMs,
    this.bridgeMs,
    this.decodeTotalMs,
    this.validateMs,
    this.locateMs,
    this.readMs,
    this.receiveMs,
    this.locked = false,
    this.rejectStage,
    this.rejectCategory,
    this.segmentIndex,
    this.packetSeed,
    this.distinct,
    this.have,
    this.need,
    this.done = false,
  });

  /// Builds a record from a diagnostics frame. [captureGap] must already be computed
  /// within one clock domain (or be `null`).
  factory FrameRecord.from(
    int index,
    ScanFrameDiagnostics d, {
    Duration? captureGap,
  }) {
    final received = d.received;
    final capture = d.capture;
    return FrameRecord(
      index: index,
      nativeFrameId: d.frameId,
      disposition: d.disposition.name,
      sinceStartMs: _ms(d.sinceStart)!,
      captureGapMs: _ms(captureGap),
      receiptLatencyMs: received != null && capture != null
          ? _ms(received.since(capture))
          : null,
      skippedBefore: d.skippedBefore,
      heldMs: _ms(d.held),
      bridgeMs: _ms(d.bridge),
      decodeTotalMs: _ms(d.decode?.total),
      validateMs: _ms(d.decode?.validate),
      locateMs: _ms(d.decode?.locate),
      readMs: _ms(d.decode?.read),
      receiveMs: _ms(d.decode?.receive),
      locked: d.locked,
      rejectStage: d.rejectStage?.name,
      rejectCategory: d.rejectCategory,
      segmentIndex: d.packet?.segmentIndex,
      packetSeed: d.packet?.packetSeed,
      distinct: d.packet?.distinct,
      have: d.have,
      need: d.need,
      done: d.done,
    );
  }

  /// Parses [toJson].
  factory FrameRecord.fromJson(Map<String, Object?> j) {
    double? d(String k) => (j[k] as num?)?.toDouble();
    int? i(String k) => (j[k] as num?)?.toInt();
    return FrameRecord(
      index: i('index') ?? 0,
      nativeFrameId: i('native_frame_id') ?? 0,
      disposition: j['disposition'] as String? ?? 'malformed',
      sinceStartMs: d('since_start_ms') ?? 0,
      captureGapMs: d('capture_gap_ms'),
      receiptLatencyMs: d('receipt_latency_ms'),
      skippedBefore: i('skipped_before') ?? 0,
      heldMs: d('held_ms'),
      bridgeMs: d('bridge_ms'),
      decodeTotalMs: d('decode_total_ms'),
      validateMs: d('decode_validate_ms'),
      locateMs: d('decode_locate_ms'),
      readMs: d('decode_read_ms'),
      receiveMs: d('decode_receive_ms'),
      locked: j['locked'] as bool? ?? false,
      rejectStage: j['reject_stage'] as String?,
      rejectCategory: j['reject_category'] as String?,
      segmentIndex: i('segment_index'),
      packetSeed: i('packet_seed'),
      distinct: j['distinct'] as bool?,
      have: i('have'),
      need: i('need'),
      done: j['done'] as bool? ?? false,
    );
  }

  /// Order of arrival within the trial.
  final int index;

  /// The native frame id.
  final int nativeFrameId;

  /// [FrameDisposition] name.
  final String disposition;

  /// Dart monotonic time from `ParticleScanner.start` to the frame event.
  final double sinceStartMs;

  /// Capture-timestamp difference to the previous frame event; only set when both
  /// timestamps are in the same native clock domain.
  final double? captureGapMs;

  /// Capture to native receipt, only when both are in the same native clock domain.
  final double? receiptLatencyMs;

  /// Frames closed unsent by the plugin since the previous frame event.
  final int skippedBefore;

  /// Native hold time, bridge round trip, Rust total and stage durations.
  final double? heldMs,
      bridgeMs,
      decodeTotalMs,
      validateMs,
      locateMs,
      readMs,
      receiveMs;

  /// Decoder result.
  final bool locked;

  /// [RejectStage] name and category for a rejected frame.
  final String? rejectStage, rejectCategory;

  /// Packet header facts, once read.
  final int? segmentIndex, packetSeed;

  /// Whether the packet was new to the receiver.
  final bool? distinct;

  /// Receiver progress after this frame.
  final int? have, need;

  /// Whether this frame completed the transfer.
  final bool done;

  /// Whether the decoder accepted a packet from this frame.
  bool get accepted =>
      disposition == 'decoded' && rejectStage == null && segmentIndex != null;

  /// JSON form (milliseconds).
  Map<String, Object?> toJson() => {
    'index': index,
    'native_frame_id': nativeFrameId,
    'disposition': disposition,
    'since_start_ms': sinceStartMs,
    'capture_gap_ms': captureGapMs,
    'receipt_latency_ms': receiptLatencyMs,
    'skipped_before': skippedBefore,
    'held_ms': heldMs,
    'bridge_ms': bridgeMs,
    'decode_total_ms': decodeTotalMs,
    'decode_validate_ms': validateMs,
    'decode_locate_ms': locateMs,
    'decode_read_ms': readMs,
    'decode_receive_ms': receiveMs,
    'locked': locked,
    'reject_stage': rejectStage,
    'reject_category': rejectCategory,
    'segment_index': segmentIndex,
    'packet_seed': packetSeed,
    'distinct': distinct,
    'have': have,
    'need': need,
    'done': done,
  };
}
