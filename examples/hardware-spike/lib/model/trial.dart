import 'device_identity.dart';
import 'measurements.dart';
import 'schema.dart';
import 'trial_config.dart';

/// Lifecycle state of a trial.
enum TrialStatus {
  /// Configured, not started. Not persisted.
  created,

  /// Started and not finished.
  running,

  /// Receiver: the decoder reported a verified payload and it equals the fixture.
  /// Sender: the tester ended the sender session.
  completed,

  /// Receiver: stopped by the tester before any completion.
  incomplete,

  /// Cancelled by the tester.
  cancelled,

  /// Failed: camera/permission/encoder error, or the decoded payload differs from the fixture.
  error;

  /// Whether no further transition is possible.
  bool get isTerminal => index >= TrialStatus.completed.index;

  /// Parses [name].
  static TrialStatus parse(Object? v) => TrialStatus.values.firstWhere(
    (s) => s.name == v,
    orElse: () => TrialStatus.error,
  );
}

/// Machine-readable reasons for terminal states.
abstract final class TrialReason {
  static const verified = 'verified';
  static const senderSessionDone = 'sender_session_done';
  static const stoppedByTester = 'stopped_by_tester';
  static const cancelledByTester = 'cancelled_by_tester';
  static const integrityMismatch = 'integrity_mismatch';
  static const scannerError = 'scanner_error';
  static const startFailed = 'start_failed';
  static const encoderFailed = 'encoder_failed';
  static const interruptedByAppExit = 'interrupted_by_app_exit';
}

/// One recorded trial (a receiver attempt or a sender session).
///
/// Durations inside are monotonic and in milliseconds; the `*Utc` fields are wall-clock
/// instants for ordering and display only and are never subtracted to get durations.
class Trial {
  /// Creates a trial.
  const Trial({
    required this.trialId,
    required this.config,
    required this.conditions,
    required this.annotations,
    required this.device,
    required this.status,
    required this.createdAtUtc,
    this.reason,
    this.reasonDetail,
    this.startedAtUtc,
    this.endedAtUtc,
    this.durationMs,
    this.sender,
    this.receiver,
  });

  /// Parses [toJson]. Throws if the structure is not a trial.
  factory Trial.fromJson(Map<String, Object?> j) {
    final cfg = j['config'];
    if (cfg is! Map || j['trial_id'] is! String) {
      throw const FormatException('not a trial record');
    }
    DateTime? time(String k) {
      final v = j[k];
      return v is String ? DateTime.tryParse(v)?.toUtc() : null;
    }

    final s = j['sender'];
    final r = j['receiver'];
    return Trial(
      trialId: j['trial_id']! as String,
      config: TrialConfig.fromJson(cfg.cast<String, Object?>()),
      conditions: Conditions.fromJson(j['conditions']),
      annotations: Annotations.fromJson(j['annotations']),
      device: DeviceIdentity.fromJson(j['device']),
      status: TrialStatus.parse(j['status']),
      reason: j['reason'] as String?,
      reasonDetail: j['reason_detail'] as String?,
      createdAtUtc: time('created_at_utc') ?? DateTime.utc(1970),
      startedAtUtc: time('started_at_utc'),
      endedAtUtc: time('ended_at_utc'),
      durationMs: (j['duration_ms'] as num?)?.toDouble(),
      sender: s is Map
          ? SenderMeasurements.fromJson(s.cast<String, Object?>())
          : null,
      receiver: r is Map
          ? ReceiverMeasurements.fromJson(r.cast<String, Object?>())
          : null,
    );
  }

  /// `T-yyyyMMdd-HHmmss-xxxx`.
  final String trialId;

  /// Settings and fixture.
  final TrialConfig config;

  /// Manual observations.
  final Conditions conditions;

  /// Variant/region/sender-hardware annotations at the time of the trial.
  final Annotations annotations;

  /// The phone that recorded this trial.
  final DeviceIdentity device;

  /// Lifecycle state, and for terminal states the reason code and optional detail.
  final TrialStatus status;
  final String? reason, reasonDetail;

  /// Wall-clock instants (UTC): record created, start tapped, ended.
  final DateTime createdAtUtc;
  final DateTime? startedAtUtc, endedAtUtc;

  /// Monotonic time from start to end, in milliseconds.
  final double? durationMs;

  /// Sender or receiver measurements, matching [TrialConfig.role].
  final SenderMeasurements? sender;
  final ReceiverMeasurements? receiver;

  /// Whether this is a receiver attempt (counts in the success denominators).
  bool get isReceiverAttempt =>
      config.role == TrialRole.receiver && status.isTerminal;

  /// A receiver trial with a verified completion that matched the fixture.
  bool get isVerifiedCompletion =>
      config.role == TrialRole.receiver &&
      status == TrialStatus.completed &&
      receiver?.integrity == Integrity.ok &&
      receiver?.completionMs.value != null;

  /// Verified completion that took at most [kTargetScanTime].
  bool get metTwoSeconds =>
      isVerifiedCompletion && receiver?.withinTwoSeconds == true;

  /// A copy with some fields replaced.
  Trial copyWith({
    TrialStatus? status,
    String? reason,
    String? reasonDetail,
    DateTime? startedAtUtc,
    DateTime? endedAtUtc,
    double? durationMs,
    SenderMeasurements? sender,
    ReceiverMeasurements? receiver,
  }) => Trial(
    trialId: trialId,
    config: config,
    conditions: conditions,
    annotations: annotations,
    device: device,
    status: status ?? this.status,
    createdAtUtc: createdAtUtc,
    reason: reason ?? this.reason,
    reasonDetail: reasonDetail ?? this.reasonDetail,
    startedAtUtc: startedAtUtc ?? this.startedAtUtc,
    endedAtUtc: endedAtUtc ?? this.endedAtUtc,
    durationMs: durationMs ?? this.durationMs,
    sender: sender ?? this.sender,
    receiver: receiver ?? this.receiver,
  );

  /// JSON form.
  Map<String, Object?> toJson() => {
    'schema_version': kTrialSchemaVersion,
    'trial_id': trialId,
    'status': status.name,
    'reason': reason,
    'reason_detail': reasonDetail,
    'created_at_utc': createdAtUtc.toIso8601String(),
    'started_at_utc': startedAtUtc?.toIso8601String(),
    'ended_at_utc': endedAtUtc?.toIso8601String(),
    'duration_ms': durationMs,
    'config': config.toJson(),
    'conditions': conditions.toJson(),
    'annotations': annotations.toJson(),
    'device': device.toJson(),
    'sender': sender?.toJson(),
    'receiver': receiver?.toJson(),
  };
}
