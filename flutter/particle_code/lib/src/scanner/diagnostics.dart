// Typed, unthrottled per-frame diagnostics of the scanner, for benchmarks and field
// reports. Contract: PROTOCOL.md, "Diagnostics".
//
// Clock domains, never mixed:
// * Dart durations ([ScanFrameDiagnostics.sinceStart], [ScanFrameDiagnostics.bridge],
//   [ScanCameraStarted.startup]) come from a Dart `Stopwatch` (monotonic) started when
//   `ParticleScanner.start` is called.
// * Native nanoseconds ([NativeTimestamp]) are in the clock named by [NativeClock]; only
//   values of the same clock may be subtracted.
// * Rust stage timings ([DecodeTimings]) are durations measured by `std::time::Instant`
//   inside one decoder call.
// * [ScanFrameDiagnostics.held] is a duration measured natively (frame receipt to release).

/// The clock a native nanosecond value belongs to.
enum NativeClock {
  /// Android `SystemClock.elapsedRealtimeNanos` (camera timestamp source REALTIME). Capture
  /// timestamps and receipt times are comparable.
  androidRealtime,

  /// Android camera timestamp source UNKNOWN: monotonic, comparable only with other capture
  /// timestamps of the same session, not with receipt times.
  androidUnknown,

  /// iOS host time clock (`mach_absolute_time` based, as `CACurrentMediaTime`). Capture
  /// timestamps and receipt times are comparable.
  iosHost,

  /// A value the plugin does not recognize.
  unknown;

  /// Parses the protocol value (`android_realtime`, `android_unknown`, `ios_host`).
  static NativeClock fromWire(Object? value) => switch (value) {
    'android_realtime' => NativeClock.androidRealtime,
    'android_unknown' => NativeClock.androidUnknown,
    'ios_host' => NativeClock.iosHost,
    _ => NativeClock.unknown,
  };

  /// The protocol value.
  String get wire => switch (this) {
    NativeClock.androidRealtime => 'android_realtime',
    NativeClock.androidUnknown => 'android_unknown',
    NativeClock.iosHost => 'ios_host',
    NativeClock.unknown => 'unknown',
  };
}

/// A native monotonic time in nanoseconds, with its clock.
class NativeTimestamp {
  /// Creates a timestamp.
  const NativeTimestamp(this.nanos, this.clock);

  /// Nanoseconds in [clock].
  final int nanos;

  /// The clock domain.
  final NativeClock clock;

  /// `this - earlier`, or `null` when the clocks differ or either is unknown.
  Duration? since(NativeTimestamp earlier) {
    if (clock != earlier.clock || clock == NativeClock.unknown) return null;
    return Duration(microseconds: (nanos - earlier.nanos) ~/ 1000);
  }
}

/// The camera buffer of one frame, as the native side delivered it.
class CameraBufferInfo {
  /// Creates buffer info.
  const CameraBufferInfo({
    required this.width,
    required this.height,
    required this.format,
    required this.yStride,
    required this.uvStride,
    required this.uvPixelStride,
    required this.rotationDegrees,
    required this.cameraId,
  });

  /// Buffer size in pixels, sensor orientation.
  final int width, height;

  /// Pixel format as the platform names it (`YUV_420_888`, `420YpCbCr8BiPlanarFullRange`).
  final String format;

  /// Row and chroma strides in bytes; chroma pixel stride (2 interleaved, 1 planar).
  final int yStride, uvStride, uvPixelStride;

  /// Clockwise rotation that makes the buffer upright on screen, or `null` if not reported.
  final int? rotationDegrees;

  /// The platform's identifier of the selected camera (not a device identifier), or `null`.
  final String? cameraId;
}

/// What the scanner did with one camera frame.
enum FrameDisposition {
  /// Fed to the decoder.
  decoded,

  /// Released undecoded: another frame was being decoded.
  skippedBusy,

  /// Released undecoded: the transfer was already done.
  skippedDone,

  /// Released undecoded: the scanner was stopping, stopped or reset.
  skippedInactive,

  /// The event was malformed; released if it had an id.
  malformed,
}

/// Decoder stage that rejected a frame.
enum RejectStage {
  /// Plane addresses, lengths or strides did not fit the dimensions.
  buffers,

  /// No code found (anchors).
  locate,

  /// Code found but header or symbols unreadable.
  read,

  /// The receiver refused the packet (CRC, other session, inconsistent header, ...).
  receiver,
}

/// Durations of the decoder stages inside one `feedCameraFrame` call (Rust `Instant`).
/// A stage that did not run is `null`.
class DecodeTimings {
  /// Creates timings.
  const DecodeTimings({
    this.validate,
    this.locate,
    this.read,
    this.receive,
    required this.total,
  });

  /// Buffer validation, locating, reading (header + symbols), receiver (RS, fountain).
  final Duration? validate, locate, read, receive;

  /// The whole call inside Rust.
  final Duration total;
}

/// The packet a decoded frame carried, once its header was read.
class PacketInfo {
  /// Creates packet info.
  const PacketInfo({
    required this.segmentIndex,
    required this.packetSeed,
    required this.distinct,
  });

  /// Segment and RaptorQ symbol id from the header.
  final int segmentIndex, packetSeed;

  /// Whether the receiver had not seen this packet before (a new distinct symbol).
  final bool distinct;
}

/// One diagnostics record.
sealed class ScanDiagnostic {
  const ScanDiagnostic();
}

/// The first camera frame after `start` arrived.
class ScanCameraStarted extends ScanDiagnostic {
  /// Creates the record.
  const ScanCameraStarted({
    required this.startup,
    required this.cameraId,
    required this.facing,
  });

  /// Dart monotonic time from calling `start` to receiving the first frame event.
  final Duration startup;

  /// Camera identifier from the `start` result, or `null`.
  final String? cameraId;

  /// `back`, `front`, `external` or `unknown`.
  final String facing;
}

/// One camera frame event, after its frame was released. Emitted for every frame event,
/// decoded or not, without rate limiting.
class ScanFrameDiagnostics extends ScanDiagnostic {
  /// Creates the record.
  const ScanFrameDiagnostics({
    required this.frameId,
    required this.disposition,
    required this.sinceStart,
    this.buffer,
    this.capture,
    this.received,
    this.skippedBefore = 0,
    this.bridge,
    this.decode,
    this.held,
    this.locked = false,
    this.rejectStage,
    this.rejectCategory,
    this.packet,
    this.have,
    this.need,
    this.segmentsDone,
    this.segmentsTotal,
    this.done = false,
  });

  /// Native frame id.
  final int frameId;

  /// What the scanner did with it.
  final FrameDisposition disposition;

  /// Dart monotonic time from `start` to receiving the frame event.
  final Duration sinceStart;

  /// The buffer, `null` for malformed events.
  final CameraBufferInfo? buffer;

  /// Capture timestamp (sensor / presentation time) and native receipt time.
  final NativeTimestamp? capture, received;

  /// Frames the native side received and closed unsent since the previous frame event,
  /// because a frame was in flight. Frames the camera pipeline dropped before delivery are
  /// not observable and not counted.
  final int skippedBefore;

  /// Dart monotonic round trip of the `feedCameraFrame` call (bridge + decode).
  final Duration? bridge;

  /// Rust stage timings, `null` when not decoded.
  final DecodeTimings? decode;

  /// Native time the frame was held (receipt to release), `null` if not reported.
  final Duration? held;

  /// Decoder result fields (only for [FrameDisposition.decoded]).
  final bool locked;

  /// Stage that rejected the frame, `null` when accepted or not decoded.
  final RejectStage? rejectStage;

  /// Rejection category, the Rust enum variant name in lowerCamelCase (`noLock`,
  /// `packetCrc`, ...), `null` when accepted or not decoded.
  final String? rejectCategory;

  /// The packet, once the header was read.
  final PacketInfo? packet;

  /// Receiver progress after this frame (sticky values as in `ScanFrame`).
  final int? have, need, segmentsDone, segmentsTotal;

  /// Whether this frame completed the transfer (payload CRC verified).
  final bool done;
}
