import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:particle_code/particle_code.dart';

import '../fixtures.dart';
import '../model/frame_record.dart';
import '../model/measurements.dart';
import '../model/observed.dart';
import '../model/schema.dart';
import '../model/stat.dart';
import '../model/trial.dart';
import 'clock.dart';
import 'scanner_port.dart';
import 'trial_factory.dart';
import 'trial_machine.dart';

/// Records one receiver trial from a scanner's events and diagnostics.
///
/// **Trial start** is the moment [start] is called (the tester tapped Start; the camera
/// start is requested right after). All trial durations (`first_accepted_packet_ms`,
/// `completion_ms`, `duration_ms`) are differences of [clock] readings taken when this
/// recorder receives the corresponding event, so they are one monotonic domain. Camera
/// startup comes from the plugin's own Dart stopwatch (`ScanCameraStarted.startup`).
/// Native capture timestamps are only ever subtracted from each other within one
/// [NativeClock].
///
/// The recorder never stops, resets or restarts the scanner because time passed. A scan
/// that crosses [kTargetScanTime] keeps running with its progress; only the tester
/// ([cancel], [stopIncomplete]), a native error or a verified completion end the trial.
class ReceiverRecorder extends ChangeNotifier {
  /// Creates a recorder for [base] (a trial in [TrialStatus.created]).
  ReceiverRecorder({
    required this.port,
    required Trial base,
    required this.fixture,
    required this.clock,
    required this.wallClock,
    this.maxFrames = 3000,
    this.settleDelay = const Duration(milliseconds: 150),
  }) : _base = base;

  /// The scanner under test.
  final ScannerPort port;

  /// The payload the sender shows.
  final Fixture fixture;

  /// Monotonic clock of the trial.
  final MonotonicClock clock;

  /// Wall clock for record timestamps only.
  final WallClock wallClock;

  /// Cap on per-frame records; counters and statistics are never capped.
  final int maxFrames;

  /// After a verified completion the recorder keeps listening this long so the diagnostics
  /// of the final frame (emitted after the frame is released) are not lost.
  final Duration settleDelay;

  final Trial _base;
  final TrialMachine _machine = TrialMachine();

  StreamSubscription<ScanEvent>? _eventSub;
  StreamSubscription<ScanDiagnostic>? _diagSub;
  final Completer<Trial> _finished = Completer<Trial>();
  bool _shutdownStarted = false;

  Duration? _startMark;
  Duration? _endMark;
  DateTime? _startedAtUtc;
  DateTime? _endedAtUtc;

  // Progress.
  int _have = 0, _need = 0, _segmentsDone = 0, _segmentsTotal = 0;
  Observed<String> _profile = const Observed.unknown();
  Observed<int> _shapeId = const Observed.unknown();
  final Map<String, int> _pairIdsSeen = {};

  // Milestones (milliseconds on the trial clock).
  double? _firstAcceptedMs;
  double? _completionMs;
  Integrity? _integrity;
  int? _receivedBytes;
  String? _receivedSha;

  // Diagnostics.
  bool _diagSeen = false;
  Observed<double> _startup = const Observed.unknown();
  Observed<String> _cameraId = const Observed.unknown();
  Observed<String> _facing = const Observed.unknown();
  CameraBuffer? _buffer;
  Observed<String> _captureClock = const Observed.unknown();
  NativeTimestamp? _prevCapture;
  int _gapPairs = 0, _crossClock = 0;
  final Map<String, List<double>> _series = {};
  final Map<String, int> _byStage = {};
  final Map<String, int> _byCategory = {};
  final List<FrameRecord> _frames = [];
  int _framesTotal = 0;
  bool _truncated = false;
  FrameCounters _counters = const FrameCounters();

  /// Current lifecycle state.
  TrialStatus get status => _machine.status;

  /// Whether the trial is running.
  bool get isRunning => _machine.isRunning;

  /// Completes with the final trial record once the trial ended and the recorder stopped
  /// listening.
  Future<Trial> get finished => _finished.future;

  /// Time since trial start (frozen when the trial ends); zero before start.
  Duration get elapsed {
    final s = _startMark;
    if (s == null) return Duration.zero;
    return (_endMark ?? clock.now) - s;
  }

  /// Fountain symbols held / needed, as last reported.
  int get have => _have;
  int get need => _need;

  /// Accepted packets that were new to the receiver.
  int get distinctPackets => _counters.distinctPackets;

  /// Whether a verified completion has been seen.
  bool get completed => _completionMs != null;

  /// Starts the trial: marks the start time, listens, then requests the camera start.
  Future<void> start() async {
    _machine.start();
    _startMark = clock.now;
    _startedAtUtc = wallClock().toUtc();
    _eventSub = port.events.listen(_onEvent);
    _diagSub = port.diagnostics.listen(_onDiagnostic);
    notifyListeners();
    try {
      await port.start();
    } catch (e) {
      if (_machine.isRunning) {
        _end();
        _machine.fail(TrialReason.startFailed, detail: '$e');
      }
      unawaited(_shutdown(Duration.zero));
    }
    notifyListeners();
  }

  /// The tester cancels. No-op unless running (or created).
  Future<void> cancel() async {
    if (_machine.status.isTerminal) return;
    if (isRunning) _end();
    _machine.cancel(TrialReason.cancelledByTester);
    notifyListeners();
    await _shutdown(Duration.zero);
  }

  /// The tester stops the scan without a completion. No-op unless running.
  Future<void> stopIncomplete() async {
    if (!isRunning) return;
    _end();
    _machine.markIncomplete(TrialReason.stoppedByTester);
    notifyListeners();
    await _shutdown(Duration.zero);
  }

  /// Marks the trial interrupted (the app is going away) and writes it as incomplete.
  Future<void> interrupt() async {
    if (!isRunning) return;
    _end();
    _machine.markIncomplete(TrialReason.interruptedByAppExit);
    notifyListeners();
    await _shutdown(Duration.zero);
  }

  void _end() {
    _endMark = clock.now;
    _endedAtUtc = wallClock().toUtc();
  }

  double _ms(Duration d) => d.inMicroseconds / 1000.0;

  void _onEvent(ScanEvent event) {
    switch (event) {
      case ScanFrame():
        if (!isRunning) return;
        _have = event.have;
        _need = event.need;
        _segmentsDone = event.segmentsDone;
        _segmentsTotal = event.segmentsTotal;
        final p = event.profile;
        if (p != null) _profile = Observed<String>.measured(p.letter);
        final s = event.shapeId;
        if (s != null) _shapeId = Observed<int>.measured(s);
        notifyListeners();
      case ScanSegment():
        break;
      case ScanDone():
        if (!isRunning) return;
        _onDone(event);
      case ScanError():
        if (!isRunning) return;
        _end();
        _machine.fail(
          TrialReason.scannerError,
          detail: '${event.code}: ${event.message}',
        );
        notifyListeners();
        unawaited(_shutdown(Duration.zero));
    }
  }

  void _onDone(ScanDone done) {
    final mark = _startMark!;
    _end();
    final completion = _ms(clock.now - mark);
    _completionMs = completion;
    _firstAcceptedMs = _firstAcceptedMs == null
        ? completion
        : (_firstAcceptedMs! > completion ? completion : _firstAcceptedMs);
    _receivedBytes = done.payload.length;
    _receivedSha = sha256Of(done.payload);
    final same =
        done.payload.length == fixture.byteCount &&
        _receivedSha == fixture.sha256Hex;
    _integrity = same ? Integrity.ok : Integrity.mismatch;
    if (same) {
      _machine.complete(TrialReason.verified);
    } else {
      _machine.fail(
        TrialReason.integrityMismatch,
        detail:
            'decoded ${done.payload.length} bytes, fixture ${fixture.id} '
            'is ${fixture.byteCount} bytes',
      );
    }
    notifyListeners();
    unawaited(_shutdown(settleDelay));
  }

  void _onDiagnostic(ScanDiagnostic d) {
    _diagSeen = true;
    switch (d) {
      case ScanCameraStarted():
        _startup = Observed<double>.measured(_ms(d.startup));
        _cameraId = Observed<String>.measuredOrUnknown(d.cameraId);
        _facing = Observed<String>.measured(d.facing);
      case ScanFrameDiagnostics():
        _onFrame(d);
    }
    notifyListeners();
  }

  void _add(String key, Duration? d) {
    if (d != null) (_series[key] ??= []).add(_ms(d));
  }

  void _onFrame(ScanFrameDiagnostics d) {
    // Delivery gap: consecutive capture timestamps, only within one native clock domain.
    final capture = d.capture;
    Duration? gap;
    if (capture != null) {
      final prev = _prevCapture;
      if (prev != null) {
        gap = capture.since(prev);
        if (gap == null) {
          _crossClock++;
        } else {
          _gapPairs++;
        }
      }
      _prevCapture = capture;
      if (_captureClock.isUnknown && capture.clock != NativeClock.unknown) {
        _captureClock = Observed<String>.measured(capture.clock.wire);
      }
    }
    _add('delivery_gap', gap);
    final received = d.received;
    if (capture != null && received != null) {
      _add('receipt_latency', received.since(capture));
    }
    _add('held', d.held);
    _add('bridge', d.bridge);
    _add('decode_total', d.decode?.total);
    _add('decode_validate', d.decode?.validate);
    _add('decode_locate', d.decode?.locate);
    _add('decode_read', d.decode?.read);
    _add('decode_receive', d.decode?.receive);

    final b = d.buffer;
    if (_buffer == null && b != null) {
      _buffer = CameraBuffer(
        width: b.width,
        height: b.height,
        format: b.format,
        yStride: b.yStride,
        uvStride: b.uvStride,
        uvPixelStride: b.uvPixelStride,
        rotationDegrees: b.rotationDegrees,
      );
      if (_cameraId.isUnknown && b.cameraId != null) {
        _cameraId = Observed<String>.measured(b.cameraId!);
      }
    }

    final session = d.sessionId;
    if (session != null) {
      _pairIdsSeen.update(
        TrialFactory.pairIdOf(session),
        (v) => v + 1,
        ifAbsent: () => 1,
      );
    }

    final rejectStage = d.rejectStage;
    if (rejectStage != null) {
      _byStage.update(rejectStage.name, (v) => v + 1, ifAbsent: () => 1);
      final cat = d.rejectCategory ?? 'unspecified';
      _byCategory.update(cat, (v) => v + 1, ifAbsent: () => 1);
    }
    final accepted =
        d.disposition == FrameDisposition.decoded &&
        rejectStage == null &&
        d.packet != null;
    if (accepted && _firstAcceptedMs == null && _startMark != null) {
      _firstAcceptedMs = _ms((_endMark ?? clock.now) - _startMark!);
    }
    final c = _counters;
    _counters = FrameCounters(
      frameEvents: c.frameEvents + 1,
      decoded: c.decoded + (d.disposition == FrameDisposition.decoded ? 1 : 0),
      skippedBusy:
          c.skippedBusy +
          (d.disposition == FrameDisposition.skippedBusy ? 1 : 0),
      skippedDone:
          c.skippedDone +
          (d.disposition == FrameDisposition.skippedDone ? 1 : 0),
      skippedInactive:
          c.skippedInactive +
          (d.disposition == FrameDisposition.skippedInactive ? 1 : 0),
      malformed:
          c.malformed + (d.disposition == FrameDisposition.malformed ? 1 : 0),
      skippedBeforeTotal: c.skippedBeforeTotal + d.skippedBefore,
      locked: c.locked + (d.locked ? 1 : 0),
      acceptedPackets: c.acceptedPackets + (accepted ? 1 : 0),
      distinctPackets:
          c.distinctPackets + (accepted && d.packet!.distinct ? 1 : 0),
    );
    _framesTotal++;
    if (_frames.length < maxFrames) {
      _frames.add(FrameRecord.from(_framesTotal - 1, d, captureGap: gap));
    } else {
      _truncated = true;
    }
  }

  Future<void> _shutdown(Duration grace) async {
    if (_shutdownStarted) return;
    _shutdownStarted = true;
    if (grace > Duration.zero) await Future<void>.delayed(grace);
    try {
      await port.stop();
    } catch (_) {
      // Best effort; the trial outcome is already decided.
    }
    await _eventSub?.cancel();
    await _diagSub?.cancel();
    _eventSub = null;
    _diagSub = null;
    if (!_finished.isCompleted) _finished.complete(trial);
    notifyListeners();
  }

  /// The measurements so far.
  ReceiverMeasurements get measurements {
    final completion = _completionMs;
    final timings = {for (final e in _series.entries) e.key: Stat.of(e.value)};
    return ReceiverMeasurements(
      diagnosticsReceived: _diagSeen,
      cameraStartupMs: _startup,
      cameraId: _cameraId,
      cameraFacing: _facing,
      buffer: _buffer,
      captureClock: _captureClock,
      firstAcceptedPacketMs: Observed<double>.measuredOrUnknown(
        _firstAcceptedMs,
      ),
      completionMs: Observed<double>.measuredOrUnknown(
        _integrity == Integrity.ok ? completion : null,
      ),
      withinTwoSeconds: _integrity == Integrity.ok && completion != null
          ? completion <= kTargetScanTime.inMicroseconds / 1000.0
          : null,
      integrity: _integrity,
      receivedBytes: _receivedBytes,
      receivedSha256: _receivedSha,
      observedProfile: _profile,
      observedShapeId: _shapeId,
      observedPairId: _pairIdsSeen.isEmpty
          ? const Observed.unknown()
          : Observed<String>.measured(
              (_pairIdsSeen.entries.toList()
                    ..sort((a, b) => b.value.compareTo(a.value)))
                  .first
                  .key,
            ),
      pairIdsSeen: Map.unmodifiable(_pairIdsSeen),
      have: _have,
      need: _need,
      segmentsDone: _segmentsDone,
      segmentsTotal: _segmentsTotal,
      counters: _counters,
      timings: timings,
      deliveryGapPairs: _gapPairs,
      crossClockPairsIgnored: _crossClock,
      rejectsByStage: Map.unmodifiable(_byStage),
      rejectsByCategory: Map.unmodifiable(_byCategory),
      frames: List.unmodifiable(_frames),
      framesTotal: _framesTotal,
      framesTruncated: _truncated,
    );
  }

  /// The trial record as of now (running or terminal).
  Trial get trial => _base.copyWith(
    status: _machine.status,
    reason: _machine.reason,
    reasonDetail: _machine.detail,
    startedAtUtc: _startedAtUtc,
    endedAtUtc: _endedAtUtc,
    durationMs: _startMark == null ? null : _ms(elapsed),
    receiver: measurements,
  );

  bool _disposed = false;

  @override
  void notifyListeners() {
    if (!_disposed) super.notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(_eventSub?.cancel());
    unawaited(_diagSub?.cancel());
    super.dispose();
  }
}
