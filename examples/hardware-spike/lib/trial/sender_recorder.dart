import '../model/measurements.dart';
import '../model/trial.dart';
import 'clock.dart';
import 'trial_machine.dart';

/// Records a sender session: the code is shown from [begin] until the tester ends it.
/// A sender session is not a scan attempt; it documents the sender configuration and
/// display so the paired receiver trials can be interpreted.
class SenderRecorder {
  /// Creates a recorder for [base] (a trial in [TrialStatus.created]).
  SenderRecorder({
    required Trial base,
    required this.clock,
    required this.wallClock,
  }) : _base = base;

  /// Monotonic clock.
  final MonotonicClock clock;

  /// Wall clock for record timestamps only.
  final WallClock wallClock;

  final Trial _base;
  final TrialMachine _machine = TrialMachine();
  Duration? _startMark;
  DateTime? _startedAtUtc;

  /// Current state.
  TrialStatus get status => _machine.status;

  /// Time shown so far.
  Duration get elapsed =>
      _startMark == null ? Duration.zero : clock.now - _startMark!;

  /// Marks the start (the code appears) and returns the running record.
  Trial begin(SenderMeasurements measurements) {
    _machine.start();
    _startMark = clock.now;
    _startedAtUtc = wallClock().toUtc();
    return _snapshot(measurements);
  }

  /// The tester ends the session.
  Trial done(SenderMeasurements measurements) {
    _machine.complete('sender_session_done');
    return _end(measurements);
  }

  /// The tester abandons the session.
  Trial cancel(SenderMeasurements measurements) {
    _machine.cancel(TrialReason.cancelledByTester);
    return _end(measurements);
  }

  /// The encoder could not be created.
  Trial fail(SenderMeasurements measurements, String detail) {
    _machine.fail(TrialReason.encoderFailed, detail: detail);
    return _end(measurements);
  }

  Trial _end(SenderMeasurements m) {
    final ms = elapsed.inMicroseconds / 1000.0;
    return _snapshot(m.withShown(ms), endedAt: wallClock().toUtc(), ms: ms);
  }

  Trial _snapshot(SenderMeasurements m, {DateTime? endedAt, double? ms}) =>
      _base.copyWith(
        status: _machine.status,
        reason: _machine.reason,
        reasonDetail: _machine.detail,
        startedAtUtc: _startedAtUtc,
        endedAtUtc: endedAt,
        durationMs: ms,
        sender: m,
      );
}
