import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../frame_spec.dart';
import '../init.dart';
import '../rust/api/dto.dart';
import 'decoder.dart';
import 'diagnostics.dart';
import 'errors.dart';
import 'events.dart';
import 'mapping.dart';

/// Controls the camera scanner: the platform captures and previews, Dart runs the decoder.
///
/// The native side (CameraX on Android, AVFoundation on iOS) only captures camera frames
/// and hands them to Dart by address on the `particle_code/camera_frames` event channel.
/// This class feeds each frame to the Rust decoder through flutter_rust_bridge (the planes
/// are read in place, never copied), releases it, and turns the result into [events].
/// See the plugin's `PROTOCOL.md`.
///
/// [start] needs `await ParticleCode.init()` to have completed and throws a [StateError]
/// otherwise.
///
/// ```dart
/// await ParticleCode.init();
/// final scanner = ParticleScanner();
/// if (await scanner.requestPermission()) {
///   scanner.events.listen((e) { if (e is ScanDone) use(e.payload); });
///   await scanner.start();
/// }
/// // ... later
/// scanner.dispose();
/// ```
class ParticleScanner {
  /// Creates a scanner. The channels default to the plugin's; override them in tests.
  ///
  /// [decoderFactory] and [clock] are test seams.
  ParticleScanner({
    MethodChannel? methodChannel,
    EventChannel? framesChannel,
    @visibleForTesting FrameDecoder Function()? decoderFactory,
    @visibleForTesting DateTime Function()? clock,
    @visibleForTesting Stopwatch Function()? stopwatchFactory,
  }) : _method = methodChannel ?? channel,
       _framesChannel = framesChannel ?? ParticleScanner.framesChannel,
       _decoderFactory = decoderFactory ?? RustFrameDecoder.new,
       _customDecoder = decoderFactory != null,
       _clock = clock ?? DateTime.now,
       _stopwatchFactory = stopwatchFactory ?? Stopwatch.new {
    _controller = StreamController<ScanEvent>.broadcast(onListen: _onListen);
  }

  /// The method channel `particle_code/scanner`.
  static const MethodChannel channel = MethodChannel('particle_code/scanner');

  /// The event channel `particle_code/camera_frames` (camera frames from the platform).
  static const EventChannel framesChannel = EventChannel(
    'particle_code/camera_frames',
  );

  /// Minimum time between two [ScanFrame]s that carry no progress change.
  static const Duration frameInterval = Duration(milliseconds: 100);

  final MethodChannel _method;
  final EventChannel _framesChannel;
  final FrameDecoder Function() _decoderFactory;
  final bool _customDecoder;
  final DateTime Function() _clock;
  final Stopwatch Function() _stopwatchFactory;
  late final StreamController<ScanEvent> _controller;
  final StreamController<ScanDiagnostic> _diagnostics =
      StreamController<ScanDiagnostic>.broadcast();

  StreamSubscription<dynamic>? _framesSub;
  FrameDecoder? _decoder;
  // Incremented by reset(): a result of an older decoder is dropped.
  int _generation = 0;
  bool _disposed = false;

  // Frames are decoded only while active, not done and not stopping.
  bool _active = false;
  bool _done = false;
  bool _stopping = false;
  Future<void>? _inFlight;

  // Diagnostics: the Dart stopwatch of the current start() and what ScanCameraStarted needs.
  Stopwatch _watch = Stopwatch();
  bool _firstFrameSeen = false;
  bool _startedEmitted = false;
  Duration? _startup;
  ({String? cameraId, String facing})? _startInfo;

  // Sticky across rejected frames.
  int _have = 0;
  int _need = 0;
  int _segmentsDone = 0;
  int _segmentsTotal = 0;
  (int, int, int, int) _lastKey = (0, 0, 0, 0);
  DateTime? _lastFrameAt;
  ScanDone? _undelivered;

  /// Broadcast stream of scanner events. Listening does not start the camera; call
  /// [start] for that. Malformed camera events arrive as a [ScanError] with code
  /// `malformed_event`; events of unknown type are ignored.
  Stream<ScanEvent> get events => _controller.stream;

  /// Broadcast stream of typed diagnostics for benchmarks: a [ScanCameraStarted] after
  /// the first frame of each [start], then one [ScanFrameDiagnostics] per camera frame
  /// event (decoded or released undecoded), never rate limited. See PROTOCOL.md,
  /// "Diagnostics", for the clock domains. Listening costs nothing when unused.
  Stream<ScanDiagnostic> get diagnostics => _diagnostics.stream;

  void _onListen() {
    // The transfer can finish while nobody listens; the payload must not get lost.
    final waiting = _undelivered;
    if (waiting != null) {
      _undelivered = null;
      scheduleMicrotask(() => _emit(waiting));
    }
  }

  void _emit(ScanEvent event) {
    if (_controller.isClosed) return;
    if (event is ScanDone && !_controller.hasListener) {
      _undelivered = event;
      return;
    }
    _controller.add(event);
  }

  /// Asks for the camera permission; returns whether it is granted.
  Future<bool> requestPermission() async =>
      await _invoke<bool>('requestPermission') ?? false;

  /// Starts the camera and decoding. Completes once the camera runs.
  ///
  /// Throws a [StateError] if `ParticleCode.init()` has not completed, and a
  /// [ParticleScannerException] with [ParticleScannerError.permissionDenied],
  /// [ParticleScannerError.noCamera] or [ParticleScannerError.cameraError].
  Future<void> start({bool torch = false}) async {
    _checkNotDisposed();
    if (!_customDecoder) ParticleCode.checkInitialized();
    _decoder ??= _decoderFactory();
    _stopping = false;
    // Listen before the camera starts: a frame nobody receives would never be released.
    _framesSub ??= _framesChannel.receiveBroadcastStream().listen(
      _onRaw,
      onError: _onStreamError,
    );
    _active = true;
    _watch = _stopwatchFactory()..start();
    _firstFrameSeen = false;
    _startedEmitted = false;
    _startup = null;
    _startInfo = null;
    try {
      final result = await _invoke<Object?>('start', {'torch': torch});
      _startInfo = _parseStartResult(result);
      _maybeEmitStarted();
    } catch (_) {
      _active = false;
      unawaited(_cancelFrames());
      rethrow;
    }
  }

  /// Stops the camera. Waits for the frame being decoded, releases it, then stops the
  /// platform. Idempotent.
  Future<void> stop() async {
    _checkNotDisposed();
    _stopping = true;
    try {
      await _settle();
      await _invoke<void>('stop');
    } finally {
      _active = false;
      _stopping = false;
      await _cancelFrames();
    }
  }

  /// Drops progress and starts a new decoder, keeping the camera running. Decoding
  /// continues after a [ScanDone]. No platform call.
  Future<void> reset() async {
    _checkNotDisposed();
    final old = _decoder;
    _generation++;
    _done = false;
    _undelivered = null;
    _clearProgress();
    if (old != null) {
      _decoder = _decoderFactory();
      _disposeWhenIdle(old);
    }
  }

  /// Turns the torch on or off. Returns whether the device has a torch.
  Future<bool> setTorch(bool on) async =>
      await _invoke<bool>('setTorch', {'on': on}) ?? false;

  /// Stops the camera (best effort), releases the decoder and closes [events].
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _active = false;
    final decoder = _decoder;
    _decoder = null;
    final sub = _framesSub;
    _framesSub = null;
    unawaited(sub?.cancel());
    unawaited(_controller.close());
    unawaited(_diagnostics.close());
    Future<void> shutdown() async {
      // Fire and forget; the platform may already be gone.
      try {
        await _method.invokeMethod<void>('stop');
      } catch (_) {}
      decoder?.dispose();
    }

    final pending = _inFlight;
    if (pending == null) {
      unawaited(shutdown());
    } else {
      unawaited(pending.then<void>((_) => shutdown()));
    }
  }

  void _checkNotDisposed() {
    if (_disposed) throw StateError('ParticleScanner used after dispose()');
  }

  Future<T?> _invoke<T>(String method, [Object? arguments]) async {
    _checkNotDisposed();
    try {
      return await _method.invokeMethod<T>(method, arguments);
    } on PlatformException catch (e) {
      throw ParticleScannerException.fromPlatform(e);
    } on MissingPluginException {
      throw const ParticleScannerException(
        ParticleScannerError.unsupported,
        'the particle_code scanner is not available on this platform',
      );
    }
  }

  Future<void> _settle() async {
    final pending = _inFlight;
    if (pending != null) await pending;
  }

  void _disposeWhenIdle(FrameDecoder decoder) {
    final pending = _inFlight;
    if (pending == null) {
      decoder.dispose();
    } else {
      unawaited(pending.then<void>((_) => decoder.dispose()));
    }
  }

  Future<void> _cancelFrames() async {
    final sub = _framesSub;
    _framesSub = null;
    await sub?.cancel();
  }

  void _clearProgress() {
    _have = 0;
    _need = 0;
    _segmentsDone = 0;
    _segmentsTotal = 0;
    _lastKey = (0, 0, 0, 0);
    _lastFrameAt = null;
  }

  // region camera frames

  void _onStreamError(Object error) {
    if (error is PlatformException) {
      _emit(ScanError(error.code, error.message ?? ''));
    } else {
      _emit(ScanError('unknown', '$error'));
    }
  }

  void _onRaw(dynamic raw) {
    if (raw is! Map) {
      _emit(
        ScanError(
          'malformed_event',
          'camera event is not a map: ${raw.runtimeType}',
        ),
      );
      return;
    }
    switch (raw['type']) {
      case 'frame':
        _onFrame(raw);
      case 'error':
        _emit(
          ScanError(
            (raw['code'] as String?) ?? 'unknown',
            (raw['message'] as String?) ?? '',
          ),
        );
      default:
        break; // forward compatibility
    }
  }

  void _onFrame(Map<Object?, Object?> map) {
    final arrived = _watch.elapsed;
    if (!_firstFrameSeen && _watch.isRunning) {
      _firstFrameSeen = true;
      _startup = arrived;
    }
    final id = map['id'];
    if (id is! int) {
      _emit(const ScanError('malformed_event', 'camera frame without an id'));
      return;
    }
    final record = _FrameRecord(id, arrived, map);
    // Defensive: native keeps one frame in flight, but never queue a second.
    final skip = !_active || _stopping
        ? FrameDisposition.skippedInactive
        : _done
        ? FrameDisposition.skippedDone
        : _inFlight != null
        ? FrameDisposition.skippedBusy
        : null;
    if (skip != null) {
      record.disposition = skip;
      _releaseAndReport(record);
      return;
    }
    final CameraFrameDto dto;
    final List<double> toPreview;
    try {
      dto = _frameDto(map);
      toPreview = _affine(map['toPreview']);
    } on FormatException catch (e) {
      record.disposition = FrameDisposition.malformed;
      _releaseAndReport(record);
      _emit(ScanError('malformed_event', e.message));
      return;
    }
    final decoder = _decoder;
    if (decoder == null) {
      record.disposition = FrameDisposition.skippedInactive;
      _releaseAndReport(record);
      return;
    }
    record.disposition = FrameDisposition.decoded;
    final future = _process(record, decoder, _generation, dto, toPreview);
    _inFlight = future;
    unawaited(
      future.whenComplete(() {
        if (identical(_inFlight, future)) _inFlight = null;
      }),
    );
  }

  Future<void> _process(
    _FrameRecord record,
    FrameDecoder decoder,
    int generation,
    CameraFrameDto dto,
    List<double> toPreview,
  ) async {
    try {
      final before = _watch.elapsed;
      final FrameResultDto result;
      try {
        result = await decoder.feed(dto);
      } finally {
        record.bridge = _watch.elapsed - before;
      }
      record.result = result;
      if (generation == _generation && !_disposed) {
        record.current = true;
        record.done = _handleResult(decoder, result, toPreview);
      }
    } catch (e, st) {
      // A bad frame is normal at first; a bridge failure is a bug worth a log.
      debugPrint('particle_code: decoding a frame failed: $e\n$st');
    } finally {
      record.held = await _releaseAndWait(record.id);
      _emitFrameDiagnostics(record);
    }
  }

  /// Releases a frame that is not decoded and reports it once released.
  void _releaseAndReport(_FrameRecord record) {
    unawaited(
      _releaseAndWait(record.id).then((held) {
        record.held = held;
        _emitFrameDiagnostics(record);
      }),
    );
  }

  /// Releases the frame; returns how long native held it, if it says.
  Future<Duration?> _releaseAndWait(int id) async {
    try {
      final result = await _method.invokeMethod<Object?>('releaseFrame', {
        'id': id,
      });
      final nanos = result is Map ? result['heldNanos'] : null;
      return nanos is int ? Duration(microseconds: nanos ~/ 1000) : null;
    } catch (_) {
      // The platform may be gone; nothing to release then.
      return null;
    }
  }

  // endregion

  // region diagnostics

  void _emitDiagnostic(ScanDiagnostic record) {
    if (_diagnostics.isClosed || !_diagnostics.hasListener) return;
    _diagnostics.add(record);
  }

  void _maybeEmitStarted() {
    final startup = _startup;
    final info = _startInfo;
    if (_startedEmitted || startup == null || info == null) return;
    _startedEmitted = true;
    _emitDiagnostic(
      ScanCameraStarted(
        startup: startup,
        cameraId: info.cameraId,
        facing: info.facing,
      ),
    );
  }

  void _emitFrameDiagnostics(_FrameRecord r) {
    if (_diagnostics.isClosed || !_diagnostics.hasListener) return;
    _maybeEmitStarted();
    final result = r.result;
    final packet = result?.packet;
    final m = r.map;
    // Progress is the scanner's sticky state; a result dropped by reset() has none.
    final progress = result == null || r.current;
    final skipped = m['skippedBefore'];
    final dropped = m['droppedByOutputBefore'];
    _emitDiagnostic(
      ScanFrameDiagnostics(
        frameId: r.id,
        disposition: r.disposition,
        sinceStart: r.arrived,
        buffer: _bufferInfo(m),
        capture: _capture(m),
        received: _received(m),
        skippedBefore: skipped is int ? skipped : 0,
        droppedByOutputBefore: dropped is int ? dropped : null,
        bridge: r.bridge,
        decode: result == null ? null : _decodeTimings(result.timings),
        held: r.held,
        locked: result?.locked ?? false,
        rejectStage: switch (result?.rejectStage) {
          null => null,
          RejectStageDto.buffers => RejectStage.buffers,
          RejectStageDto.locate => RejectStage.locate,
          RejectStageDto.read => RejectStage.read,
          RejectStageDto.receiver => RejectStage.receiver,
        },
        rejectCategory: result?.rejectCategory?.name,
        packet: packet == null
            ? null
            : PacketInfo(
                segmentIndex: packet.segmentIndex,
                packetSeed: packet.packetSeed,
                distinct: packet.distinct,
              ),
        sessionId: result?.sessionId,
        have: progress ? _have : null,
        need: progress ? _need : null,
        segmentsDone: progress ? _segmentsDone : null,
        segmentsTotal: progress ? _segmentsTotal : null,
        done: r.done,
      ),
    );
  }

  // endregion

  // region camera frames (results)

  /// Returns whether this frame completed the transfer.
  bool _handleResult(
    FrameDecoder decoder,
    FrameResultDto result,
    List<double> toPreview,
  ) {
    var status = ScanStatus.rejected;
    Uint8List? donePayload;
    switch (result.progress) {
      case ProgressDto_Accepted(
        :final have,
        :final need,
        :final segmentsDone,
        :final segmentsTotal,
      ):
        status = ScanStatus.accepted;
        _have = have;
        _need = need;
        _segmentsDone = segmentsDone;
        _segmentsTotal = segmentsTotal;
      case ProgressDto_Done(:final payload):
        status = ScanStatus.done;
        donePayload = payload;
        if (_need > 0) _have = _need;
        if (_segmentsTotal > 0) _segmentsDone = _segmentsTotal;
      case ProgressDto_Rejected():
        break;
    }

    final ready = status == ScanStatus.rejected ? null : decoder.takeReady();
    final sessionId = donePayload == null ? -1 : (decoder.sessionId() ?? -1);

    final now = _clock();
    final key = (_have, _need, _segmentsDone, _segmentsTotal);
    final changed = key != _lastKey || status == ScanStatus.done;
    final last = _lastFrameAt;
    if (changed || last == null || now.difference(last) >= frameInterval) {
      _lastFrameAt = now;
      _lastKey = key;
      _emit(_scanFrame(result, status, toPreview));
    }
    if (ready != null && ready.isNotEmpty) _emit(ScanSegment(ready));
    if (donePayload != null) {
      _done = true;
      _emit(ScanDone(donePayload, sessionId));
    }
    return donePayload != null;
  }

  ScanFrame _scanFrame(
    FrameResultDto r,
    ScanStatus status,
    List<double> toPreview,
  ) {
    final live = r.liveness;
    return ScanFrame(
      status: status,
      locked: r.locked,
      profile: r.profile == null
          ? null
          : CodeProfile.fromCode(r.profile!.index),
      shapeId: r.shapeId,
      have: _have,
      need: _need,
      segmentsDone: _segmentsDone,
      segmentsTotal: _segmentsTotal,
      liveness: switch (live) {
        LivenessDto_Read(:final consistent, :final seedLow, :final rolling) =>
          ScanLiveness(
            consistent: consistent,
            seedLow: seedLow,
            rolling: rolling,
          ),
        _ => null,
      },
      anchors: mapAnchorsToPreview(r.anchors, toPreview),
      sharpness: r.sharpness,
      hint: r.hint,
      hintCode: scanHintFor(r.hintCode),
      rejectReason: switch (r.progress) {
        ProgressDto_Rejected(:final reason) => r.rejectReason ?? reason,
        _ => r.rejectReason,
      },
    );
  }

  // endregion
}

/// What the scanner knows about one camera frame event, until its diagnostics are emitted.
class _FrameRecord {
  _FrameRecord(this.id, this.arrived, this.map);

  final int id;
  final Duration arrived;
  final Map<Object?, Object?> map;
  FrameDisposition disposition = FrameDisposition.malformed;
  FrameResultDto? result;
  Duration? bridge;
  Duration? held;
  bool done = false;

  /// Whether the result was applied (no reset happened while decoding).
  bool current = false;
}

({String? cameraId, String facing}) _parseStartResult(Object? result) {
  final map = result is Map ? result : const <Object?, Object?>{};
  final id = map['cameraId'];
  final facing = map['facing'];
  const known = {'back', 'front', 'external', 'unknown'};
  return (
    cameraId: id is String ? id : null,
    facing: facing is String && known.contains(facing) ? facing : 'unknown',
  );
}

CameraBufferInfo? _bufferInfo(Map<Object?, Object?> m) {
  final width = m['width'], height = m['height'];
  final yStride = m['yStride'], uvStride = m['uvStride'];
  final uvPixelStride = m['uvPixelStride'];
  if (width is! int ||
      height is! int ||
      yStride is! int ||
      uvStride is! int ||
      uvPixelStride is! int) {
    return null;
  }
  final format = m['format'], rotation = m['rotationDegrees'];
  final cameraId = m['cameraId'];
  return CameraBufferInfo(
    width: width,
    height: height,
    format: format is String ? format : 'unknown',
    yStride: yStride,
    uvStride: uvStride,
    uvPixelStride: uvPixelStride,
    rotationDegrees: rotation is int ? rotation : null,
    cameraId: cameraId is String ? cameraId : null,
  );
}

NativeTimestamp? _capture(Map<Object?, Object?> m) {
  final nanos = m['timestampNanos'];
  if (nanos is! int) return null;
  return NativeTimestamp(nanos, NativeClock.fromWire(m['timestampClock']));
}

/// Receipt times are Android `elapsedRealtimeNanos` or iOS host time, whatever the capture
/// clock of the sensor is (PROTOCOL.md, "Diagnostics").
NativeTimestamp? _received(Map<Object?, Object?> m) {
  final nanos = m['receivedNanos'];
  if (nanos is! int) return null;
  final clock = switch (NativeClock.fromWire(m['timestampClock'])) {
    NativeClock.androidRealtime ||
    NativeClock.androidUnknown => NativeClock.androidRealtime,
    NativeClock.iosHost => NativeClock.iosHost,
    NativeClock.unknown => NativeClock.unknown,
  };
  return NativeTimestamp(nanos, clock);
}

DecodeTimings _decodeTimings(DecodeTimingsDto t) {
  Duration? us(int? v) => v == null ? null : Duration(microseconds: v);
  return DecodeTimings(
    validate: us(t.validateUs),
    locate: us(t.locateUs),
    read: us(t.readUs),
    receive: us(t.receiveUs),
    total: Duration(microseconds: t.totalUs),
  );
}

CameraFrameDto _frameDto(Map<Object?, Object?> m) {
  int i(String key) {
    final v = m[key];
    if (v is int) return v;
    throw FormatException('camera frame field "$key" is not an int: $v');
  }

  return CameraFrameDto(
    width: i('width'),
    height: i('height'),
    yAddress: i('yAddress'),
    yLen: i('yLen'),
    uAddress: i('uAddress'),
    uLen: i('uLen'),
    vAddress: i('vAddress'),
    vLen: i('vLen'),
    yStride: i('yStride'),
    uvStride: i('uvStride'),
    uvPixelStride: i('uvPixelStride'),
  );
}

List<double> _affine(Object? raw) {
  if (raw is List && raw.length == 6 && raw.every((e) => e is num)) {
    return [for (final e in raw) (e as num).toDouble()];
  }
  throw FormatException('camera frame "toPreview" is not 6 numbers: $raw');
}
