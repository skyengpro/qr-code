import 'dart:convert';
import 'dart:io';

import 'package:archive/archive.dart';
import 'package:crypto/crypto.dart';

import '../model/device_identity.dart';
import '../model/schema.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';
import '../trial/clock.dart';
import 'aggregation.dart';
import 'csv.dart';
import 'report.dart';

/// A report bundle written to disk.
class ExportResult {
  /// Creates a result.
  const ExportResult({
    required this.file,
    required this.createdAtUtc,
    required this.trialCount,
    required this.digest,
  });

  /// Parses [toJson].
  factory ExportResult.fromJson(Map<String, Object?> j) => ExportResult(
    file: File(j['path']! as String),
    createdAtUtc: DateTime.parse(j['created_at_utc']! as String),
    trialCount: (j['trial_count']! as num).toInt(),
    digest: j['digest']! as String,
  );

  /// The zip bundle.
  final File file;

  /// When it was written.
  final DateTime createdAtUtc;

  /// Number of trials in it.
  final int trialCount;

  /// Fingerprint of the trial set (ids, states, end times) the bundle contains.
  final String digest;

  /// JSON form for the persisted state.
  Map<String, Object?> toJson() => {
    'path': file.path,
    'created_at_utc': createdAtUtc.toIso8601String(),
    'trial_count': trialCount,
    'digest': digest,
  };
}

/// Fingerprint of a trial set: changes when a trial is added or changes state.
String trialsDigest(Iterable<Trial> trials) {
  final lines =
      trials
          .map(
            (t) =>
                '${t.trialId}|${t.status.name}|${t.endedAtUtc?.toIso8601String()}',
          )
          .toList()
        ..sort();
  return sha256.convert(utf8.encode(lines.join('\n'))).toString();
}

/// Writes report bundles. Pure file IO: it needs no network.
///
/// A bundle is a zip with `report.json` (the machine-readable report), `trials.csv`
/// (one row per attempt, every outcome), `summary.csv` (one row per configuration
/// group), `frames.csv` (per-frame diagnostics) and `README.txt`.
class ReportExporter {
  /// Writes into [directory] (created if missing).
  ReportExporter({required this.directory, required this.wallClock});

  /// Output directory (the app documents directory in production).
  final Directory directory;

  /// Wall clock for the file name and `generated_at_utc`.
  final WallClock wallClock;

  /// Exports [trials] with the exporting phone's [device] and the current [annotations].
  Future<ExportResult> export({
    required List<Trial> trials,
    required DeviceIdentity device,
    required Annotations annotations,
  }) async {
    final now = wallClock().toUtc();
    final report = buildReport(
      trials: trials,
      device: device,
      annotations: annotations,
      generatedAtUtc: now,
    );
    final archive = Archive()
      ..addFile(
        ArchiveFile.string(
          'report.json',
          const JsonEncoder.withIndent('  ').convert(report),
        ),
      )
      ..addFile(ArchiveFile.string('trials.csv', trialsCsv(trials)))
      ..addFile(
        ArchiveFile.string('summary.csv', summaryCsv(aggregate(trials))),
      )
      ..addFile(ArchiveFile.string('frames.csv', framesCsv(trials)))
      ..addFile(ArchiveFile.string('README.txt', bundleReadme(now)));
    final bytes = ZipEncoder().encodeBytes(archive);

    await directory.create(recursive: true);
    final stamp = _stamp(now);
    var file = File('${directory.path}/particle-spike-report-$stamp.zip');
    for (var n = 2; await file.exists(); n++) {
      file = File('${directory.path}/particle-spike-report-$stamp-$n.zip');
    }
    final tmp = File('${file.path}.part');
    await tmp.writeAsBytes(bytes, flush: true);
    await tmp.rename(file.path);
    return ExportResult(
      file: file,
      createdAtUtc: now,
      trialCount: trials.length,
      digest: trialsDigest(trials),
    );
  }

  static String _stamp(DateTime t) {
    String two(int v) => v.toString().padLeft(2, '0');
    return '${t.year}${two(t.month)}${two(t.day)}-'
        '${two(t.hour)}${two(t.minute)}${two(t.second)}Z';
  }
}

/// Reads a bundle back: the parsed report and the CSV texts. Throws on a malformed zip.
({ParsedReport report, Map<String, String> files}) readBundle(
  List<int> zipBytes,
) {
  final archive = ZipDecoder().decodeBytes(zipBytes);
  final files = <String, String>{
    for (final f in archive.files)
      if (f.isFile) f.name: utf8.decode(f.readBytes() ?? const []),
  };
  final json = files['report.json'];
  if (json == null) throw const FormatException('bundle has no report.json');
  final report = parseReport((jsonDecode(json) as Map).cast<String, Object?>());
  return (report: report, files: files);
}

/// The text of `README.txt` inside a bundle.
String bundleReadme(DateTime generatedAtUtc) =>
    '''
Particle Code hardware spike report
Generated: ${generatedAtUtc.toIso8601String()}
Schema: $kReportSchemaName v$kReportSchemaVersion

Files
  report.json  Machine-readable report: device, annotations, units, definitions,
               aggregation and every trial with its measurements.
  trials.csv   One row per trial, every outcome (completed, incomplete, cancelled,
               error), unknown values empty.
  summary.csv  One row per configuration group. "within_2s_of_all_attempts" divides by
               all terminal receiver attempts; latency columns are verified
               completions only.
  frames.csv   Per-frame diagnostics of receiver trials (capped per trial).

Privacy
  Contains only benchmark data: synthetic fixture ids and sizes, device model, OS and app
  versions, timings, counters and tester-typed annotations. No credentials, keys, camera
  images, persistent device identifiers or unrelated logs.

Caveat
  Results from a few devices, or from successful trials only, are not evidence of
  CEMAC-wide compatibility or of two-second reliability.
''';
