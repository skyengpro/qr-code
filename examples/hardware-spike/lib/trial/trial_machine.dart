import '../model/trial.dart';

/// Thrown for a transition the lifecycle forbids.
class IllegalTrialTransition extends StateError {
  /// Creates the error.
  IllegalTrialTransition(TrialStatus from, TrialStatus to)
    : super('illegal trial transition ${from.name} -> ${to.name}');
}

/// The trial lifecycle: `created -> running -> completed | incomplete | cancelled | error`
/// (and `created -> cancelled | error` if the trial never started). Terminal states are
/// final. There is no timeout transition: only an explicit tester action, a native error
/// or a verified completion ends a trial.
class TrialMachine {
  /// Starts in [TrialStatus.created].
  TrialMachine();

  TrialStatus _status = TrialStatus.created;
  String? _reason;
  String? _detail;

  /// Current state.
  TrialStatus get status => _status;

  /// Reason code of the terminal state.
  String? get reason => _reason;

  /// Free-text detail of the terminal state.
  String? get detail => _detail;

  /// Whether the trial is running.
  bool get isRunning => _status == TrialStatus.running;

  /// `created -> running`.
  void start() => _move(TrialStatus.running);

  /// `running -> completed`.
  void complete(String reason, {String? detail}) =>
      _move(TrialStatus.completed, reason, detail);

  /// `running -> incomplete`.
  void markIncomplete(String reason, {String? detail}) =>
      _move(TrialStatus.incomplete, reason, detail);

  /// `created | running -> cancelled`.
  void cancel(String reason, {String? detail}) =>
      _move(TrialStatus.cancelled, reason, detail);

  /// `created | running -> error`.
  void fail(String reason, {String? detail}) =>
      _move(TrialStatus.error, reason, detail);

  void _move(TrialStatus to, [String? reason, String? detail]) {
    final from = _status;
    final ok = switch (to) {
      TrialStatus.running => from == TrialStatus.created,
      TrialStatus.completed ||
      TrialStatus.incomplete => from == TrialStatus.running,
      TrialStatus.cancelled ||
      TrialStatus.error => from == TrialStatus.created || isRunning,
      TrialStatus.created => false,
    };
    if (!ok) throw IllegalTrialTransition(from, to);
    _status = to;
    _reason = reason;
    _detail = detail;
  }
}
