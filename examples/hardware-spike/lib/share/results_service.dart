import 'package:flutter/foundation.dart';

import '../model/device_identity.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';
import '../report/exporter.dart';
import '../storage/trial_store.dart';
import '../trial/clock.dart';
import 'result_sharer.dart';

/// The last share attempt, persisted so it is still visible after a restart.
class ShareRecord {
  /// Creates a record.
  const ShareRecord({
    required this.outcome,
    required this.atUtc,
    required this.bundlePath,
  });

  /// Parses [toJson].
  factory ShareRecord.fromJson(Map<String, Object?> j) => ShareRecord(
    outcome: ShareOutcome(
      ShareOutcomeKind.parse(j['outcome']),
      j['error'] as String?,
    ),
    atUtc: DateTime.tryParse('${j['at_utc']}') ?? DateTime.utc(1970),
    bundlePath: j['bundle_path'] as String? ?? '',
  );

  /// What happened.
  final ShareOutcome outcome;

  /// When.
  final DateTime atUtc;

  /// The bundle that was offered.
  final String bundlePath;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'outcome': outcome.kind.name,
    'error': outcome.error,
    'at_utc': atUtc.toIso8601String(),
    'bundle_path': bundlePath,
  };
}

/// Export and send workflows over the stored trials.
///
/// Export is pure file IO and works offline. Send exports (or reuses the up-to-date
/// bundle) and hands it to the share sheet; whatever happens, the bundle stays on disk
/// for another try, and the outcome is described as a handoff, never as delivery.
class ResultsService extends ChangeNotifier {
  /// Creates the service.
  ResultsService({
    required this.store,
    required this.exporter,
    required this.sharer,
    required this.wallClock,
  });

  /// Where the last export/share state is kept.
  final TrialStore store;

  /// Writes bundles.
  final ReportExporter exporter;

  /// Share sheet.
  final ResultSharer sharer;

  /// Wall clock for records.
  final WallClock wallClock;

  ExportResult? _lastExport;
  ShareRecord? _lastShare;
  String? _lastError;
  bool _busy = false;

  /// The newest bundle on disk, if any.
  ExportResult? get lastExport => _lastExport;

  /// The last share attempt.
  ShareRecord? get lastShare => _lastShare;

  /// The last export failure, cleared by the next success.
  String? get lastError => _lastError;

  /// Whether an export or share is in progress.
  bool get busy => _busy;

  /// Loads the persisted state; forgets a bundle whose file is gone.
  Future<void> init() async {
    final s = await store.loadResultsState();
    final e = s['last_export'];
    if (e is Map) {
      try {
        final r = ExportResult.fromJson(e.cast<String, Object?>());
        if (await r.file.exists()) _lastExport = r;
      } on Object {
        // Ignore a damaged state entry.
      }
    }
    final sh = s['last_share'];
    if (sh is Map) {
      _lastShare = ShareRecord.fromJson(sh.cast<String, Object?>());
    }
    notifyListeners();
  }

  Future<void> _persist() async {
    try {
      await store.saveResultsState({
        'last_export': _lastExport?.toJson(),
        'last_share': _lastShare?.toJson(),
      });
    } on Object {
      // The bundle itself is on disk; only the remembered state is lost.
    }
  }

  /// Writes a fresh bundle of [trials]. Returns `null` (and sets [lastError]) if the
  /// disk write failed.
  Future<ExportResult?> exportResults({
    required List<Trial> trials,
    required DeviceIdentity device,
    required Annotations annotations,
  }) async {
    if (_busy) return null;
    _busy = true;
    notifyListeners();
    try {
      final r = await exporter.export(
        trials: trials,
        device: device,
        annotations: annotations,
      );
      _lastExport = r;
      _lastError = null;
      await _persist();
      return r;
    } on Object catch (e) {
      _lastError = 'Export failed: $e';
      return null;
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  /// Sends the results through the share sheet. Reuses the last bundle when it still
  /// matches [trials]; otherwise exports first.
  Future<ShareRecord?> sendResults({
    required List<Trial> trials,
    required DeviceIdentity device,
    required Annotations annotations,
  }) async {
    var bundle = _lastExport;
    final fresh =
        bundle != null &&
        await bundle.file.exists() &&
        bundle.digest == trialsDigest(trials);
    if (!fresh) {
      bundle = await exportResults(
        trials: trials,
        device: device,
        annotations: annotations,
      );
      if (bundle == null) return null;
    }
    if (_busy) return null;
    _busy = true;
    notifyListeners();
    try {
      final file = bundle.file;
      final outcome = await sharer.share(
        file,
        subject: 'Particle Code hardware spike report',
      );
      _lastShare = ShareRecord(
        outcome: outcome,
        atUtc: wallClock().toUtc(),
        bundlePath: file.path,
      );
      await _persist();
      return _lastShare;
    } finally {
      _busy = false;
      notifyListeners();
    }
  }
}
