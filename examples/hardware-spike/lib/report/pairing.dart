// Plain Dart (no Flutter): also used by tool/pair_reports.dart. Works on the raw
// `report.json` maps, so it reads reports of any app version.

import '../fixtures.dart';
import 'csv_codec.dart';

/// How a receiver trial was matched to a sender trial.
enum PairMatch {
  /// The receiver read the sender's pair id from the code (`observed_pair_id`).
  pairId,

  /// No pair id was read (or the report predates it): the only sender trial on another
  /// phone that was showing a code while the receiver scanned.
  time,

  /// Several sender trials fit; not paired.
  ambiguous,

  /// No sender trial fits.
  unpaired,
}

/// One receiver trial and the sender trial it was paired with, if any.
class TrialPair {
  /// Creates a pair.
  const TrialPair({
    required this.receiver,
    required this.sender,
    required this.match,
    this.candidates = 0,
  });

  /// The raw receiver and sender trial maps.
  final Map<String, Object?> receiver;
  final Map<String, Object?>? sender;

  /// How they were matched.
  final PairMatch match;

  /// Sender trials that fitted (for [PairMatch.ambiguous]).
  final int candidates;
}

/// Clocks of two phones disagree a little; a sender trial still counts as showing the
/// code if its interval overlaps the receiver's widened by this much.
const Duration pairingClockSlack = Duration(seconds: 5);

/// Collects the trials of several reports, keeping one copy of each trial id (a trial
/// exported twice keeps the copy that ended last, so a finished record wins over the
/// `running` one).
List<Map<String, Object?>> collectTrials(
  Iterable<Map<String, Object?>> reports,
) {
  final byId = <String, Map<String, Object?>>{};
  for (final report in reports) {
    for (final t in _list(report['trials'])) {
      final trial = _map(t);
      final id = trial['trial_id'];
      if (id is! String) continue;
      final kept = byId[id];
      if (kept == null || _endKey(trial).compareTo(_endKey(kept)) > 0) {
        byId[id] = trial;
      }
    }
  }
  final all = byId.values.toList()
    ..sort(
      (a, b) => _str(a['created_at_utc']).compareTo(_str(b['created_at_utc'])),
    );
  return all;
}

String _endKey(Map<String, Object?> t) =>
    '${t['ended_at_utc'] ?? ''}|${t['status'] == 'running' ? 0 : 1}';

/// Pairs every receiver trial in [trials] with a sender trial.
///
/// By pair id when the receiver read one from the code: the sender trial whose code
/// carried it, the one closest in time if a 16-bit id repeats. Otherwise by time: the
/// single sender trial on another device whose interval overlaps the receiver's (see
/// [pairingClockSlack]).
List<TrialPair> pairTrials(List<Map<String, Object?>> trials) {
  final senders = [
    for (final t in trials)
      if (_role(t) == 'sender') t,
  ];
  final pairs = <TrialPair>[];
  for (final r in trials) {
    if (_role(r) != 'receiver') continue;
    final observed = _observed(_map(r['receiver'])['observed_pair_id']);
    if (observed is String && observed.isNotEmpty) {
      final byId = [
        for (final s in senders)
          if (_pairIdOnScreen(s) == observed) s,
      ];
      if (byId.isNotEmpty) {
        byId.sort((a, b) => _distance(a, r).compareTo(_distance(b, r)));
        pairs.add(
          TrialPair(receiver: r, sender: byId.first, match: PairMatch.pairId),
        );
        continue;
      }
    }
    final overlapping = [
      for (final s in senders)
        if (_device(s) != _device(r) && _overlaps(s, r)) s,
    ];
    pairs.add(switch (overlapping.length) {
      0 => TrialPair(receiver: r, sender: null, match: PairMatch.unpaired),
      1 => TrialPair(
        receiver: r,
        sender: overlapping.single,
        match: PairMatch.time,
      ),
      _ => TrialPair(
        receiver: r,
        sender: null,
        match: PairMatch.ambiguous,
        candidates: overlapping.length,
      ),
    });
  }
  return pairs;
}

/// Columns of `paired.csv`.
const List<String> pairedHeader = [
  'pair_id',
  'match',
  'receiver_trial_id',
  'sender_trial_id',
  'receiver_device',
  'receiver_os',
  'sender_device',
  'sender_os',
  'fixture_id',
  'payload_bytes',
  'profile',
  'shape',
  'fps',
  'static',
  'ecc',
  'repair_per_pass',
  'motion',
  'status',
  'reason',
  'integrity_vs_sender',
  'completion_ms',
  'within_2s',
  'first_accepted_packet_ms',
  'frame_events',
  'locked_frames',
  'accepted_packets',
  'distinct_packets',
  'distance_cm',
  'code_size_mm',
  'lighting',
  'screen_protector',
  'sender_code_side_physical_px',
  'sender_device_pixel_ratio',
  'sender_dots_across',
  'declared_differs',
  'receiver_started_at_utc',
  'sender_started_at_utc',
];

/// One `paired.csv` row. Code settings are the sender's (what was really shown);
/// conditions are the receiver's, else the sender's.
List<Object?> pairedRow(TrialPair p) {
  final r = p.receiver;
  final s = p.sender;
  final rc = _map(r['config']);
  final sc = s == null ? const <String, Object?>{} : _map(s['config']);
  final code = s == null ? rc : sc;
  final m = _map(r['receiver']);
  final counters = _map(m['counters']);
  final sm = s == null ? const <String, Object?>{} : _map(s['sender']);
  Object? cond(String k) =>
      _observed(_map(r['conditions'])[k]) ??
      (s == null ? null : _observed(_map(s['conditions'])[k]));
  return [
    s == null ? _observed(m['observed_pair_id']) : _pairIdOnScreen(s),
    p.match == PairMatch.ambiguous
        ? 'ambiguous (${p.candidates} senders)'
        : p.match.name,
    r['trial_id'],
    s?['trial_id'],
    _device(r),
    _observed(_map(r['device'])['os_version']),
    s == null ? null : _device(s),
    s == null ? null : _observed(_map(s['device'])['os_version']),
    code['fixture_id'],
    code['payload_bytes'],
    code['profile'],
    code['shape'],
    code['fps'],
    code['static'],
    code['ecc'],
    code['repair_per_pass'],
    code['motion'],
    r['status'],
    r['reason'],
    s == null ? null : integrityVsSender(r, s),
    _observed(m['completion_ms']),
    m['within_two_seconds'],
    _observed(m['first_accepted_packet_ms']),
    counters['frame_events'],
    counters['locked'],
    counters['accepted_packets'],
    counters['distinct_packets'],
    cond('distance_cm'),
    cond('code_size_mm'),
    cond('lighting'),
    cond('screen_protector'),
    _observed(sm['code_side_physical_px']),
    _observed(sm['device_pixel_ratio']),
    _observed(sm['dots_across']),
    s == null ? null : declaredDiffers(rc, sc).join(' '),
    r['started_at_utc'],
    s?['started_at_utc'],
  ];
}

/// `paired.csv`.
String pairedCsv(List<TrialPair> pairs) =>
    encodeCsv(pairedHeader, [for (final p in pairs) pairedRow(p)]);

/// Whether the receiver decoded the sender's fixture: `ok`, `mismatch`, or `none`
/// when nothing was decoded. Judged against what the sender showed, not against what
/// the receiver declared (a wrong declaration is not a decoding error).
String integrityVsSender(Map<String, Object?> r, Map<String, Object?> s) {
  final m = _map(r['receiver']);
  final sha = m['received_sha256'];
  if (sha is! String) return 'none';
  final fixture = Fixtures.byId(_str(_map(s['config'])['fixture_id']));
  return fixture != null && fixture.sha256Hex == sha ? 'ok' : 'mismatch';
}

/// Code settings the receiver declared differently from what the sender showed.
List<String> declaredDiffers(
  Map<String, Object?> receiverConfig,
  Map<String, Object?> senderConfig,
) => [
  for (final k in const [
    'fixture_id',
    'profile',
    'shape_id',
    'fps',
    'static',
    'repair_per_pass',
    'motion',
  ])
    if (receiverConfig[k] != senderConfig[k]) k,
];

/// Counts by [PairMatch].
Map<PairMatch, int> matchCounts(List<TrialPair> pairs) => {
  for (final m in PairMatch.values) m: pairs.where((p) => p.match == m).length,
};

// -----------------------------------------------------------------------------------

String? _role(Map<String, Object?> t) => _map(t['config'])['role'] as String?;

String? _pairIdOnScreen(Map<String, Object?> s) =>
    (_observed(_map(s['sender'])['pair_id_on_screen']) as String?) ??
    _map(s['config'])['pair_id'] as String?;

String? _device(Map<String, Object?> t) =>
    _observed(_map(t['device'])['model_identifier']) as String?;

DateTime? _time(Object? v) => v is String ? DateTime.tryParse(v) : null;

({DateTime start, DateTime end})? _interval(Map<String, Object?> t) {
  final start = _time(t['started_at_utc']) ?? _time(t['created_at_utc']);
  final end = _time(t['ended_at_utc']) ?? start;
  return start == null || end == null ? null : (start: start, end: end);
}

bool _overlaps(Map<String, Object?> a, Map<String, Object?> b) {
  final x = _interval(a);
  final y = _interval(b);
  if (x == null || y == null) return false;
  return !x.end.add(pairingClockSlack).isBefore(y.start) &&
      !y.end.add(pairingClockSlack).isBefore(x.start);
}

/// How far apart two trials are: 0 when they overlap, else the gap.
Duration _distance(Map<String, Object?> a, Map<String, Object?> b) {
  final x = _interval(a);
  final y = _interval(b);
  if (x == null || y == null) return const Duration(days: 36500);
  if (_overlaps(a, b)) return Duration.zero;
  return x.end.isBefore(y.start)
      ? y.start.difference(x.end)
      : x.start.difference(y.end);
}

/// The `value` of an `Observed` JSON object (`{value, source}`), or a plain value.
Object? _observed(Object? v) => v is Map ? v['value'] : v;

Map<String, Object?> _map(Object? v) =>
    v is Map ? v.cast<String, Object?>() : const <String, Object?>{};

List<Object?> _list(Object? v) => v is List ? v : const [];

String _str(Object? v) => v is String ? v : '';
