import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code_hardware_spike/model/measurements.dart';
import 'package:particle_code_hardware_spike/model/observed.dart';
import 'package:particle_code_hardware_spike/model/schema.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/model/trial_config.dart';
import 'package:particle_code_hardware_spike/report/aggregation.dart';
import 'package:particle_code_hardware_spike/report/csv.dart';
import 'package:particle_code_hardware_spike/report/exporter.dart';
import 'package:particle_code_hardware_spike/report/report.dart';

import 'support/fakes.dart';
import 'support/trials.dart';

/// 1 verified within 2 s, 1 verified after 2 s, then one of each failure kind, plus a
/// sender session that must not count as an attempt.
List<Trial> sampleTrials() => [
  receiverTrial(
    'T-1',
    TrialStatus.completed,
    completionMs: 1500,
    integrity: Integrity.ok,
    reason: 'verified',
  ),
  receiverTrial(
    'T-2',
    TrialStatus.completed,
    completionMs: 3000,
    integrity: Integrity.ok,
    reason: 'verified',
    conditions: Conditions(
      notes: Observed<String>.manual('has, "quotes"\nand newline'),
    ),
  ),
  receiverTrial('T-3', TrialStatus.incomplete, reason: 'stopped_by_tester'),
  receiverTrial('T-4', TrialStatus.cancelled, reason: 'cancelled_by_tester'),
  receiverTrial(
    'T-5',
    TrialStatus.error,
    integrity: Integrity.mismatch,
    reason: 'integrity_mismatch',
  ),
  senderTrial('T-6'),
];

void main() {
  group('aggregation', () {
    test('the denominator is every terminal receiver attempt', () {
      final a = aggregate(sampleTrials());
      expect(a.overall.attempts, 5);
      expect(a.overall.withinTwoSeconds, 1);
      expect(a.overall.eventualCompletions, 2);
      expect(a.overall.incomplete, 1);
      expect(a.overall.cancelled, 1);
      expect(a.overall.error, 1);
      expect(a.overall.withinTwoSecondsRate, 1 / 5);
      expect(a.senderSessions, 1);
    });

    test('latency percentiles use successful completions only', () {
      final l = aggregate(sampleTrials()).overall.latency;
      expect(l.count, 2);
      expect(l.p50, 1500);
      expect(l.p90, 3000);
      expect(l.max, 3000);
    });

    test('a group of only failures has no latency and rate zero', () {
      final a = aggregate([
        receiverTrial('T-1', TrialStatus.incomplete),
        receiverTrial('T-2', TrialStatus.cancelled),
      ]);
      expect(a.overall.latency.count, 0);
      expect(a.overall.latency.p50, isNull);
      expect(a.overall.withinTwoSecondsRate, 0);
    });

    test('running trials are not attempts yet', () {
      final running = receiverTrial('T-1', TrialStatus.running);
      expect(aggregate([running]).overall.attempts, 0);
    });

    test('trials split into groups by configuration and receiver model', () {
      final a = aggregate([
        receiverTrial(
          'T-1',
          TrialStatus.completed,
          completionMs: 1000,
          integrity: Integrity.ok,
        ),
        receiverTrial(
          'T-2',
          TrialStatus.completed,
          completionMs: 1000,
          integrity: Integrity.ok,
          model: 'iPhone14,5',
        ),
      ]);
      expect(a.groups.length, 2);
      expect(a.groups.every((g) => g.attempts == 1), isTrue);
    });
  });

  group('report.json', () {
    test(
      'serializes and reparses with version, units, definitions and caveats',
      () {
        final trials = sampleTrials();
        final json = buildReport(
          trials: trials,
          device: fakeDevice(),
          annotations: const Annotations(),
          generatedAtUtc: DateTime.utc(2026, 2, 3, 4, 5, 6),
        );
        final decoded = (jsonDecode(jsonEncode(json)) as Map)
            .cast<String, Object?>();
        expect(decoded['schema'], kReportSchemaName);
        expect(decoded['schema_version'], kReportSchemaVersion);
        expect((decoded['app'] as Map)['source_revision'], isNotNull);
        expect(decoded['units'], isNotEmpty);
        expect(decoded['target_scan_time_ms'], 2000);
        expect(
          (decoded['caveats'] as List).join(' '),
          contains('not evidence of CEMAC-wide compatibility'),
        );
        final parsed = parseReport(decoded);
        expect(parsed.trials.length, trials.length);
        expect(
          jsonEncode(parsed.trials.map((t) => t.toJson()).toList()),
          jsonEncode(trials.map((t) => t.toJson()).toList()),
        );
        expect(
          parsed.exportingDevice.text('model_identifier'),
          'Pixel 9 Pro XL',
        );
        final counts = decoded['counts'] as Map;
        expect(counts['receiver_attempts'], 5);
      },
    );

    test('contains no identifiers, keys or image data', () {
      final json = buildReport(
        trials: sampleTrials(),
        device: fakeDevice(),
        annotations: const Annotations(),
        generatedAtUtc: DateTime.utc(2026),
      );
      // The prose sections name the excluded things on purpose.
      json
        ..remove('privacy_exclusions')
        ..remove('caveats')
        ..remove('definitions');
      final text = jsonEncode(json).toLowerCase();
      for (final banned in [
        'android_id',
        'identifierforvendor',
        'serial_number',
        '"key"',
        'session_key',
        'image',
      ]) {
        expect(text.contains(banned), isFalse, reason: banned);
      }
    });

    test('rejects other schemas and newer versions', () {
      expect(() => parseReport({'schema': 'x'}), throwsFormatException);
      expect(
        () => parseReport({
          'schema': kReportSchemaName,
          'schema_version': kReportSchemaVersion + 1,
        }),
        throwsFormatException,
      );
    });
  });

  group('CSV', () {
    test('encode/parse handles commas, quotes and newlines', () {
      final text = encodeCsv(
        ['a', 'b'],
        [
          ['x,y', 'he said "hi"'],
          ['line1\nline2', null],
        ],
      );
      expect(parseCsv(text), [
        ['a', 'b'],
        ['x,y', 'he said "hi"'],
        ['line1\nline2', ''],
      ]);
    });

    test('trials.csv has one row per trial and matches the records', () {
      final trials = sampleTrials();
      final rows = parseCsv(trialsCsv(trials));
      expect(rows.first, trialsHeader);
      expect(rows.length, trials.length + 1);
      int col(String name) => trialsHeader.indexOf(name);
      for (var i = 0; i < trials.length; i++) {
        final t = trials[i];
        final row = rows[i + 1];
        expect(row.length, trialsHeader.length);
        expect(row[col('trial_id')], t.trialId);
        expect(row[col('status')], t.status.name);
        expect(row[col('reason')], t.reason);
        expect(row[col('role')], t.config.role.name);
        expect(row[col('fixture_id')], t.config.fixtureId);
        expect(row[col('payload_bytes')], '${t.config.payloadBytes}');
        expect(row[col('device_model')], 'Pixel 9 Pro XL');
        final r = t.receiver;
        if (r == null) {
          expect(row[col('completion_ms')], '');
          continue;
        }
        expect(row[col('sender_hardware')], 'iPhone 13');
        final c = r.completionMs.value;
        expect(row[col('completion_ms')], c == null ? '' : '$c');
        expect(row[col('within_2s')], r.withinTwoSeconds?.toString() ?? '');
        expect(row[col('integrity')], r.integrity?.name ?? '');
        expect(row[col('frame_events')], '${r.counters.frameEvents}');
        expect(row[col('camera_startup_ms')], '${r.cameraStartupMs.value}');
      }
      // The incomplete attempt is present with empty completion fields.
      final inc = rows.firstWhere((r) => r[0] == 'T-3');
      expect(inc[col('status')], 'incomplete');
      expect(inc[col('completion_ms')], '');
      expect(inc[col('within_2s')], '');
      // Notes with commas, quotes and newlines survive.
      expect(
        rows.firstWhere((r) => r[0] == 'T-2')[col('notes')],
        'has, "quotes"\nand newline',
      );
    });

    test('summary.csv matches the aggregation', () {
      final trials = sampleTrials();
      final agg = aggregate(trials);
      final rows = parseCsv(summaryCsv(agg));
      expect(rows.first, summaryHeader);
      expect(rows.length, agg.groups.length + 1);
      int col(String name) => summaryHeader.indexOf(name);
      final g = agg.groups.single;
      final row = rows[1];
      expect(row[col('attempts')], '${g.attempts}');
      expect(row[col('attempts')], '5');
      expect(row[col('completed_within_2s')], '1');
      expect(row[col('eventual_completions')], '2');
      expect(row[col('incomplete')], '1');
      expect(row[col('cancelled')], '1');
      expect(row[col('error')], '1');
      expect(double.parse(row[col('within_2s_of_all_attempts')]), 0.2);
      expect(row[col('latency_successful_p50_ms')], '1500.0');
      expect(row[col('latency_successful_p90_ms')], '3000.0');
      expect(row[col('latency_successful_count')], '2');
      expect(row[col('receiver_model')], 'Pixel 9 Pro XL');
    });

    test('summary rows add up to the trials.csv statuses', () {
      final trials = sampleTrials();
      final byStatus = <String, int>{};
      final role = trialsHeader.indexOf('role');
      final status = trialsHeader.indexOf('status');
      for (final r in parseCsv(trialsCsv(trials)).skip(1)) {
        if (r[role] != 'receiver') continue;
        byStatus.update(r[status], (v) => v + 1, ifAbsent: () => 1);
      }
      final s = aggregate(trials).overall;
      expect(byStatus['completed'], s.eventualCompletions);
      expect(byStatus['incomplete'], s.incomplete);
      expect(byStatus['cancelled'], s.cancelled);
      expect(byStatus['error'], s.error);
    });

    test('frames.csv rows match the recorded frames', () {
      final trials = [
        receiverTrial('T-1', TrialStatus.incomplete, withFrames: true),
      ];
      final rows = parseCsv(framesCsv(trials));
      expect(rows.first, framesHeader);
      expect(rows.length, 3);
      expect(rows[1][0], 'T-1');
      expect(rows[1][framesHeader.indexOf('capture_gap_ms')], '33.25');
      expect(rows[2][framesHeader.indexOf('disposition')], 'skippedBusy');
    });
  });

  group('export bundle (offline)', () {
    late Directory dir;
    setUp(() => dir = Directory.systemTemp.createTempSync('spike_export'));
    tearDown(() => dir.deleteSync(recursive: true));

    test(
      'writes a zip that can be reopened and parsed, with no network',
      () async {
        final previous = HttpOverrides.current;
        HttpOverrides.global = _NoNetwork();
        addTearDown(() => HttpOverrides.global = previous);

        final exporter = ReportExporter(
          directory: Directory('${dir.path}/reports'),
          wallClock: () => DateTime.utc(2026, 2, 3, 4, 5, 6),
        );
        final trials = sampleTrials();
        final result = await exporter.export(
          trials: trials,
          device: fakeDevice(),
          annotations: const Annotations(),
        );
        expect(result.file.existsSync(), isTrue);
        expect(
          result.file.path,
          endsWith('particle-spike-report-20260203-040506Z.zip'),
        );
        expect(result.trialCount, trials.length);

        final bundle = readBundle(result.file.readAsBytesSync());
        expect(bundle.files.keys.toSet(), {
          'report.json',
          'trials.csv',
          'summary.csv',
          'frames.csv',
          'README.txt',
        });
        expect(bundle.report.trials.length, trials.length);
        expect(bundle.report.schemaVersion, kReportSchemaVersion);
        // The CSVs inside the zip match the raw records of the parsed report.
        expect(bundle.files['trials.csv'], trialsCsv(bundle.report.trials));
        expect(
          bundle.files['summary.csv'],
          summaryCsv(aggregate(bundle.report.trials)),
        );
        expect(bundle.files['README.txt'], contains('not evidence'));
      },
    );

    test('an empty result set still exports a valid bundle', () async {
      final exporter = ReportExporter(
        directory: dir,
        wallClock: () => DateTime.utc(2026),
      );
      final r = await exporter.export(
        trials: const [],
        device: fakeDevice(),
        annotations: const Annotations(),
      );
      final b = readBundle(r.file.readAsBytesSync());
      expect(b.report.trials, isEmpty);
      expect(parseCsv(b.files['trials.csv']!).length, 1);
    });

    test(
      'two exports in the same second do not overwrite each other',
      () async {
        final exporter = ReportExporter(
          directory: dir,
          wallClock: () => DateTime.utc(2026),
        );
        Future<ExportResult> go() => exporter.export(
          trials: const [],
          device: fakeDevice(),
          annotations: const Annotations(),
        );
        final a = await go();
        final b = await go();
        expect(a.file.path, isNot(b.file.path));
        expect(a.file.existsSync() && b.file.existsSync(), isTrue);
      },
    );

    test('the digest changes when a trial is added or changes state', () {
      final a = trialsDigest(sampleTrials());
      expect(trialsDigest(sampleTrials()), a);
      expect(trialsDigest(sampleTrials().take(3)), isNot(a));
    });
  });
}

class _NoNetwork extends HttpOverrides {
  @override
  HttpClient createHttpClient(SecurityContext? context) =>
      throw StateError('network used during export');
}
