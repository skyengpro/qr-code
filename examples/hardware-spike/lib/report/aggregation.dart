import '../model/stat.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';

/// Aggregate of the receiver attempts of one configuration group.
///
/// Every terminal receiver attempt is in [attempts]: the two-second success rate divides
/// by all of them, including incomplete, cancelled and failed ones. The latency
/// statistics describe verified completions only and say nothing about the others.
class GroupSummary {
  /// Creates a summary.
  const GroupSummary({
    required this.key,
    required this.fields,
    required this.attempts,
    required this.withinTwoSeconds,
    required this.eventualCompletions,
    required this.incomplete,
    required this.cancelled,
    required this.error,
    required this.latency,
  });

  /// Group key (the joined [fields]).
  final String key;

  /// Configuration fields that define the group.
  final Map<String, String> fields;

  /// All terminal receiver attempts.
  final int attempts;

  /// Verified completions at or under two seconds.
  final int withinTwoSeconds;

  /// Verified completions at any time (a superset of [withinTwoSeconds]).
  final int eventualCompletions;

  /// Attempts by non-success outcome (error includes integrity mismatches).
  final int incomplete, cancelled, error;

  /// Completion latency in milliseconds over verified completions only.
  final Stat latency;

  /// `withinTwoSeconds / attempts`, or `null` without attempts.
  double? get withinTwoSecondsRate =>
      attempts == 0 ? null : withinTwoSeconds / attempts;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'group': fields,
    'attempts': attempts,
    'completed_within_2s': withinTwoSeconds,
    'eventual_completions': eventualCompletions,
    'incomplete': incomplete,
    'cancelled': cancelled,
    'error': error,
    'within_2s_of_all_attempts': withinTwoSecondsRate,
    'completion_latency_ms_successful_only': latency.toJson(),
  };
}

/// The aggregation of a set of trials.
class Aggregation {
  /// Creates an aggregation.
  const Aggregation({
    required this.groups,
    required this.overall,
    required this.senderSessions,
  });

  /// Per-configuration summaries, sorted by key.
  final List<GroupSummary> groups;

  /// All receiver attempts together (not a population estimate).
  final GroupSummary overall;

  /// Completed or otherwise ended sender sessions (not scan attempts).
  final int senderSessions;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'groups': [for (final g in groups) g.toJson()],
    'overall': overall.toJson(),
    'sender_sessions': senderSessions,
  };
}

/// Fields that define a configuration group.
Map<String, String> groupFields(Trial t) {
  final c = t.config;
  return {
    'receiver_model': t.device.text('model_identifier') ?? 'unavailable',
    'receiver_os': t.device.text('os_version') ?? 'unavailable',
    'sender_hardware': t.annotations.senderHardware.value ?? 'unavailable',
    'fixture': c.fixtureId,
    'profile': c.profile,
    'shape': c.shapeName,
    'fps': '${c.fps}',
    'ecc': c.ecc,
    'repair_per_pass': '${c.repairPerPass}',
    'motion': c.motion.name,
  };
}

GroupSummary _summarize(
  String key,
  Map<String, String> fields,
  List<Trial> ts,
) {
  var within = 0, eventual = 0, incomplete = 0, cancelled = 0, error = 0;
  final latencies = <double>[];
  for (final t in ts) {
    if (t.isVerifiedCompletion) {
      eventual++;
      latencies.add(t.receiver!.completionMs.value!);
      if (t.metTwoSeconds) within++;
    } else {
      switch (t.status) {
        case TrialStatus.incomplete:
          incomplete++;
        case TrialStatus.cancelled:
          cancelled++;
        default:
          // `error`, and a `completed` receiver record without a verified match.
          error++;
      }
    }
  }
  return GroupSummary(
    key: key,
    fields: fields,
    attempts: ts.length,
    withinTwoSeconds: within,
    eventualCompletions: eventual,
    incomplete: incomplete,
    cancelled: cancelled,
    error: error,
    latency: Stat.of(latencies),
  );
}

/// Aggregates [trials]: receiver attempts that reached a terminal state, grouped by
/// configuration; sender sessions are only counted.
Aggregation aggregate(Iterable<Trial> trials) {
  final attempts = trials.where((t) => t.isReceiverAttempt).toList();
  final byKey = <String, List<Trial>>{};
  final fieldsOf = <String, Map<String, String>>{};
  for (final t in attempts) {
    final fields = groupFields(t);
    final key = fields.values.join('|');
    (byKey[key] ??= []).add(t);
    fieldsOf[key] = fields;
  }
  final keys = byKey.keys.toList()..sort();
  return Aggregation(
    groups: [for (final k in keys) _summarize(k, fieldsOf[k]!, byKey[k]!)],
    overall: _summarize('all', const {}, attempts),
    senderSessions: trials
        .where((t) => t.config.role == TrialRole.sender && t.status.isTerminal)
        .length,
  );
}
