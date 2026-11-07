import 'frame_record.dart';
import 'observed.dart';
import 'stat.dart';

/// What the sender screen could measure about its display and encoder.
class SenderMeasurements {
  /// Creates measurements.
  const SenderMeasurements({
    this.logicalWidth = const Observed.unknown(),
    this.logicalHeight = const Observed.unknown(),
    this.devicePixelRatio = const Observed.unknown(),
    this.codeSideLogicalPx = const Observed.unknown(),
    this.codeSidePhysicalPx = const Observed.unknown(),
    this.dotsAcross = const Observed.unknown(),
    this.dotCount = const Observed.unknown(),
    this.blockSizeBytes = const Observed.unknown(),
    this.segmentCount = const Observed.unknown(),
    this.framesPerPass = const Observed.unknown(),
    this.frameHalfSidePitches = const Observed.unknown(),
    this.quietZonePitches = const Observed.unknown(),
    this.brightness = const Observed.unknown(),
    this.brightnessPolicy = defaultBrightnessPolicy,
    this.shownMs = const Observed.unknown(),
    this.pairIdOnScreen = const Observed.unknown(),
  });

  /// Parses [toJson].
  factory SenderMeasurements.fromJson(Map<String, Object?> j) =>
      SenderMeasurements(
        logicalWidth: Observed.fromJson<double>(
          j['logical_width_px'],
          decode: decodeDouble,
        ),
        logicalHeight: Observed.fromJson<double>(
          j['logical_height_px'],
          decode: decodeDouble,
        ),
        devicePixelRatio: Observed.fromJson<double>(
          j['device_pixel_ratio'],
          decode: decodeDouble,
        ),
        codeSideLogicalPx: Observed.fromJson<double>(
          j['code_side_logical_px'],
          decode: decodeDouble,
        ),
        codeSidePhysicalPx: Observed.fromJson<double>(
          j['code_side_physical_px'],
          decode: decodeDouble,
        ),
        dotsAcross: Observed.fromJson<int>(j['dots_across'], decode: decodeInt),
        dotCount: Observed.fromJson<int>(j['dot_count'], decode: decodeInt),
        blockSizeBytes: Observed.fromJson<int>(
          j['block_size_bytes'],
          decode: decodeInt,
        ),
        segmentCount: Observed.fromJson<int>(
          j['segment_count'],
          decode: decodeInt,
        ),
        framesPerPass: Observed.fromJson<int>(
          j['frames_per_pass'],
          decode: decodeInt,
        ),
        frameHalfSidePitches: Observed.fromJson<double>(
          j['frame_half_side_pitches'],
          decode: decodeDouble,
        ),
        quietZonePitches: Observed.fromJson<double>(
          j['quiet_zone_pitches'],
          decode: decodeDouble,
        ),
        brightness: Observed.fromJson<double>(
          j['brightness'],
          decode: decodeDouble,
        ),
        brightnessPolicy:
            j['brightness_policy'] as String? ?? defaultBrightnessPolicy,
        shownMs: Observed.fromJson<double>(j['shown_ms'], decode: decodeDouble),
        pairIdOnScreen: Observed.fromJson<String>(j['pair_id_on_screen']),
      );

  /// The plugin forces maximum brightness while a code is shown and does not read the
  /// level back, so the level is not measured.
  static const String defaultBrightnessPolicy =
      'maximum, forced by ParticleCodeView while shown; level not read back';

  /// Logical screen size and device pixel ratio (Flutter `MediaQuery`).
  final Observed<double> logicalWidth, logicalHeight, devicePixelRatio;

  /// Side of the code square on screen.
  final Observed<double> codeSideLogicalPx, codeSidePhysicalPx;

  /// Lattice and encoder facts (profile dots across, `EncoderInfo`).
  final Observed<int> dotsAcross, dotCount, blockSizeBytes;
  final Observed<int> segmentCount, framesPerPass;
  final Observed<double> frameHalfSidePitches, quietZonePitches;

  /// Screen brightness level (0..1); unknown unless a platform reports it.
  final Observed<double> brightness;

  /// What is known about brightness handling.
  final String brightnessPolicy;

  /// How long the code was shown (monotonic).
  final Observed<double> shownMs;

  /// The session id the encoder put in every header, as `P-xxxx`: what a receiver reads
  /// as its `observed_pair_id`. Equals the trial's pair id unless that was not `P-xxxx`.
  final Observed<String> pairIdOnScreen;

  /// A copy with a new [shownMs].
  SenderMeasurements withShown(double ms) => SenderMeasurements(
    logicalWidth: logicalWidth,
    logicalHeight: logicalHeight,
    devicePixelRatio: devicePixelRatio,
    codeSideLogicalPx: codeSideLogicalPx,
    codeSidePhysicalPx: codeSidePhysicalPx,
    dotsAcross: dotsAcross,
    dotCount: dotCount,
    blockSizeBytes: blockSizeBytes,
    segmentCount: segmentCount,
    framesPerPass: framesPerPass,
    frameHalfSidePitches: frameHalfSidePitches,
    quietZonePitches: quietZonePitches,
    brightness: brightness,
    brightnessPolicy: brightnessPolicy,
    shownMs: Observed<double>.measured(ms),
    pairIdOnScreen: pairIdOnScreen,
  );

  /// JSON form.
  Map<String, Object?> toJson() => {
    'logical_width_px': logicalWidth.toJson(),
    'logical_height_px': logicalHeight.toJson(),
    'device_pixel_ratio': devicePixelRatio.toJson(),
    'code_side_logical_px': codeSideLogicalPx.toJson(),
    'code_side_physical_px': codeSidePhysicalPx.toJson(),
    'dots_across': dotsAcross.toJson(),
    'dot_count': dotCount.toJson(),
    'block_size_bytes': blockSizeBytes.toJson(),
    'segment_count': segmentCount.toJson(),
    'frames_per_pass': framesPerPass.toJson(),
    'frame_half_side_pitches': frameHalfSidePitches.toJson(),
    'quiet_zone_pitches': quietZonePitches.toJson(),
    'brightness': brightness.toJson(),
    'brightness_policy': brightnessPolicy,
    'shown_ms': shownMs.toJson(),
    'pair_id_on_screen': pairIdOnScreen.toJson(),
  };
}

/// The camera buffer the native side delivered (first frame).
class CameraBuffer {
  /// Creates buffer info.
  const CameraBuffer({
    required this.width,
    required this.height,
    required this.format,
    required this.yStride,
    required this.uvStride,
    required this.uvPixelStride,
    this.rotationDegrees,
  });

  /// Parses [toJson].
  factory CameraBuffer.fromJson(Map<String, Object?> j) => CameraBuffer(
    width: (j['width'] as num?)?.toInt() ?? 0,
    height: (j['height'] as num?)?.toInt() ?? 0,
    format: j['format'] as String? ?? '',
    yStride: (j['y_stride_bytes'] as num?)?.toInt() ?? 0,
    uvStride: (j['uv_stride_bytes'] as num?)?.toInt() ?? 0,
    uvPixelStride: (j['uv_pixel_stride'] as num?)?.toInt() ?? 0,
    rotationDegrees: (j['rotation_degrees'] as num?)?.toInt(),
  );

  /// Buffer size in pixels, platform pixel format name, strides in bytes.
  final int width, height, yStride, uvStride, uvPixelStride;

  /// Pixel format name.
  final String format;

  /// Clockwise rotation to upright, if reported.
  final int? rotationDegrees;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'width': width,
    'height': height,
    'format': format,
    'y_stride_bytes': yStride,
    'uv_stride_bytes': uvStride,
    'uv_pixel_stride': uvPixelStride,
    'rotation_degrees': rotationDegrees,
  };
}

/// Frame counters from the diagnostics stream.
class FrameCounters {
  /// Creates counters.
  const FrameCounters({
    this.frameEvents = 0,
    this.decoded = 0,
    this.skippedBusy = 0,
    this.skippedDone = 0,
    this.skippedInactive = 0,
    this.malformed = 0,
    this.skippedBeforeTotal = 0,
    this.locked = 0,
    this.acceptedPackets = 0,
    this.distinctPackets = 0,
  });

  /// Parses [toJson].
  factory FrameCounters.fromJson(Object? json) {
    final j = json is Map ? json : const <String, Object?>{};
    int i(String k) => (j[k] as num?)?.toInt() ?? 0;
    return FrameCounters(
      frameEvents: i('frame_events'),
      decoded: i('decoded'),
      skippedBusy: i('skipped_busy'),
      skippedDone: i('skipped_done'),
      skippedInactive: i('skipped_inactive'),
      malformed: i('malformed'),
      skippedBeforeTotal: i('skipped_before_total'),
      locked: i('locked'),
      acceptedPackets: i('accepted_packets'),
      distinctPackets: i('distinct_packets'),
    );
  }

  /// Frame events reported (offered to Dart), and by disposition.
  final int frameEvents,
      decoded,
      skippedBusy,
      skippedDone,
      skippedInactive,
      malformed;

  /// Frames the plugin received and closed unsent, between frame events.
  final int skippedBeforeTotal;

  /// Decoded frames where the anchors were found.
  final int locked;

  /// Frames from which the decoder accepted a packet, and how many were new.
  final int acceptedPackets, distinctPackets;

  /// Frames the plugin received from the camera: events plus silently skipped ones.
  /// Frames the camera pipeline dropped before delivery are not included (unobservable).
  int get pluginReceived => frameEvents + skippedBeforeTotal;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'frame_events': frameEvents,
    'decoded': decoded,
    'skipped_busy': skippedBusy,
    'skipped_done': skippedDone,
    'skipped_inactive': skippedInactive,
    'malformed': malformed,
    'skipped_before_total': skippedBeforeTotal,
    'plugin_received': pluginReceived,
    'locked': locked,
    'accepted_packets': acceptedPackets,
    'distinct_packets': distinctPackets,
  };
}

/// Result of comparing the decoded payload with the selected fixture.
enum Integrity {
  /// Byte-for-byte equal.
  ok,

  /// The decoder reported completion but the bytes differ.
  mismatch;

  /// Parses [name].
  static Integrity? parse(Object? v) => v == 'ok'
      ? ok
      : v == 'mismatch'
      ? mismatch
      : null;
}

/// Everything the receiver recorded.
class ReceiverMeasurements {
  /// Creates measurements.
  const ReceiverMeasurements({
    this.diagnosticsReceived = false,
    this.cameraStartupMs = const Observed.unknown(),
    this.cameraId = const Observed.unknown(),
    this.cameraFacing = const Observed.unknown(),
    this.buffer,
    this.captureClock = const Observed.unknown(),
    this.firstAcceptedPacketMs = const Observed.unknown(),
    this.completionMs = const Observed.unknown(),
    this.withinTwoSeconds,
    this.integrity,
    this.receivedBytes,
    this.receivedSha256,
    this.observedProfile = const Observed.unknown(),
    this.observedShapeId = const Observed.unknown(),
    this.observedPairId = const Observed.unknown(),
    this.pairIdsSeen = const {},
    this.have = 0,
    this.need = 0,
    this.segmentsDone = 0,
    this.segmentsTotal = 0,
    this.counters = const FrameCounters(),
    this.timings = const {},
    this.deliveryGapPairs = 0,
    this.crossClockPairsIgnored = 0,
    this.rejectsByStage = const {},
    this.rejectsByCategory = const {},
    this.frames = const [],
    this.framesTotal = 0,
    this.framesTruncated = false,
  });

  /// Parses [toJson].
  factory ReceiverMeasurements.fromJson(Map<String, Object?> j) {
    Map<String, int> counts(Object? raw) => raw is Map
        ? {for (final e in raw.entries) '${e.key}': (e.value as num).toInt()}
        : const {};
    final t = j['timings_ms'];
    final b = j['camera_buffer'];
    final f = j['frames'];
    return ReceiverMeasurements(
      diagnosticsReceived: j['diagnostics_received'] as bool? ?? false,
      cameraStartupMs: Observed.fromJson<double>(
        j['camera_startup_ms'],
        decode: decodeDouble,
      ),
      cameraId: Observed.fromJson<String>(j['camera_id']),
      cameraFacing: Observed.fromJson<String>(j['camera_facing']),
      buffer: b is Map
          ? CameraBuffer.fromJson(b.cast<String, Object?>())
          : null,
      captureClock: Observed.fromJson<String>(j['capture_clock']),
      firstAcceptedPacketMs: Observed.fromJson<double>(
        j['first_accepted_packet_ms'],
        decode: decodeDouble,
      ),
      completionMs: Observed.fromJson<double>(
        j['completion_ms'],
        decode: decodeDouble,
      ),
      withinTwoSeconds: j['within_two_seconds'] as bool?,
      integrity: Integrity.parse(j['integrity']),
      receivedBytes: (j['received_bytes'] as num?)?.toInt(),
      receivedSha256: j['received_sha256'] as String?,
      observedProfile: Observed.fromJson<String>(j['observed_profile']),
      observedShapeId: Observed.fromJson<int>(
        j['observed_shape_id'],
        decode: decodeInt,
      ),
      observedPairId: Observed.fromJson<String>(j['observed_pair_id']),
      pairIdsSeen: counts(j['pair_ids_seen']),
      have: (j['have'] as num?)?.toInt() ?? 0,
      need: (j['need'] as num?)?.toInt() ?? 0,
      segmentsDone: (j['segments_done'] as num?)?.toInt() ?? 0,
      segmentsTotal: (j['segments_total'] as num?)?.toInt() ?? 0,
      counters: FrameCounters.fromJson(j['counters']),
      timings: t is Map
          ? {for (final e in t.entries) '${e.key}': Stat.fromJson(e.value)}
          : const {},
      deliveryGapPairs: (j['delivery_gap_pairs'] as num?)?.toInt() ?? 0,
      crossClockPairsIgnored:
          (j['cross_clock_pairs_ignored'] as num?)?.toInt() ?? 0,
      rejectsByStage: counts(j['rejects_by_stage']),
      rejectsByCategory: counts(j['rejects_by_category']),
      frames: f is List
          ? [
              for (final e in f)
                FrameRecord.fromJson((e as Map).cast<String, Object?>()),
            ]
          : const [],
      framesTotal: (j['frames_total'] as num?)?.toInt() ?? 0,
      framesTruncated: j['frames_truncated'] as bool? ?? false,
    );
  }

  /// Whether any diagnostics record arrived. When `false`, the plugin emitted none and
  /// the counters and timings below are not measurements but zeros.
  final bool diagnosticsReceived;

  /// Time from the camera start request to the first frame event (`ScanCameraStarted`).
  final Observed<double> cameraStartupMs;

  /// Selected camera (platform camera id, not a device identifier) and facing.
  final Observed<String> cameraId, cameraFacing;

  /// First frame's buffer geometry and format.
  final CameraBuffer? buffer;

  /// Native clock domain of the capture timestamps.
  final Observed<String> captureClock;

  /// Trial start to the first accepted packet / verified completion (trial monotonic
  /// clock, see README).
  final Observed<double> firstAcceptedPacketMs, completionMs;

  /// `completionMs <= 2000`; `null` when there is no verified completion. Computed after
  /// the fact, never used to stop the scan.
  final bool? withinTwoSeconds;

  /// Payload comparison with the fixture.
  final Integrity? integrity;

  /// Size and SHA-256 of the decoded payload.
  final int? receivedBytes;
  final String? receivedSha256;

  /// Profile and shape the receiver read from the code's header.
  final Observed<String> observedProfile;
  final Observed<int> observedShapeId;

  /// The sender's pair id, read from the code: the session id of the headers this trial
  /// read, as `P-xxxx` (the one seen in the most frames). Unknown if no header was read.
  final Observed<String> observedPairId;

  /// Frames per pair id whose header was read; more than one key means another code
  /// was in view too.
  final Map<String, int> pairIdsSeen;

  /// Last known receiver progress.
  final int have, need, segmentsDone, segmentsTotal;

  /// Frame counters.
  final FrameCounters counters;

  /// Duration statistics by name (`delivery_gap`, `held`, `bridge`, `decode_total`, ...).
  final Map<String, Stat> timings;

  /// Capture-timestamp pairs used for `delivery_gap`, and pairs skipped because their
  /// clock domains differed or were unknown.
  final int deliveryGapPairs, crossClockPairsIgnored;

  /// Rejected-frame counts by decoder stage and by category.
  final Map<String, int> rejectsByStage, rejectsByCategory;

  /// Per-frame records (capped), the total number seen, and whether the cap cut them.
  final List<FrameRecord> frames;
  final int framesTotal;
  final bool framesTruncated;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'diagnostics_received': diagnosticsReceived,
    'camera_startup_ms': cameraStartupMs.toJson(),
    'camera_id': cameraId.toJson(),
    'camera_facing': cameraFacing.toJson(),
    'camera_buffer': buffer?.toJson(),
    'capture_clock': captureClock.toJson(),
    'first_accepted_packet_ms': firstAcceptedPacketMs.toJson(),
    'completion_ms': completionMs.toJson(),
    'within_two_seconds': withinTwoSeconds,
    'integrity': integrity?.name,
    'received_bytes': receivedBytes,
    'received_sha256': receivedSha256,
    'observed_profile': observedProfile.toJson(),
    'observed_shape_id': observedShapeId.toJson(),
    'observed_pair_id': observedPairId.toJson(),
    'pair_ids_seen': pairIdsSeen,
    'have': have,
    'need': need,
    'segments_done': segmentsDone,
    'segments_total': segmentsTotal,
    'counters': counters.toJson(),
    'camera_pipeline_drops': const Observed<int>.unknown().toJson(),
    'timings_ms': {for (final e in timings.entries) e.key: e.value.toJson()},
    'delivery_gap_pairs': deliveryGapPairs,
    'cross_clock_pairs_ignored': crossClockPairsIgnored,
    'rejects_by_stage': rejectsByStage,
    'rejects_by_category': rejectsByCategory,
    'frames_total': framesTotal,
    'frames_truncated': framesTruncated,
    'frames': [for (final f in frames) f.toJson()],
  };
}
