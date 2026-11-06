import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code_hardware_spike/model/measurements.dart';
import 'package:particle_code_hardware_spike/model/observed.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/model/trial_config.dart';
import 'package:particle_code_hardware_spike/report/exporter.dart';
import 'package:particle_code_hardware_spike/share/result_sharer.dart';
import 'package:particle_code_hardware_spike/share/results_service.dart';
import 'package:particle_code_hardware_spike/storage/trial_store.dart';

import 'support/fakes.dart';
import 'support/trials.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory dir;
  setUp(() => dir = Directory.systemTemp.createTempSync('spike_store'));
  tearDown(() => dir.deleteSync(recursive: true));

  group('TrialStore', () {
    test('completed and incomplete trials survive a restart', () async {
      final a = TrialStore(dir);
      final done = receiverTrial(
        'T-1',
        TrialStatus.completed,
        completionMs: 1800,
        integrity: Integrity.ok,
      );
      final partial = receiverTrial(
        'T-2',
        TrialStatus.incomplete,
        withFrames: true,
      );
      await a.save(done);
      await a.save(partial);

      final b = TrialStore(dir); // a new instance, as after a restart
      final loaded = await b.loadAll();
      expect(loaded.trials.map((t) => t.trialId), ['T-1', 'T-2']);
      expect(loaded.unreadable, 0);
      expect(loaded.trials[0].receiver!.completionMs.value, 1800);
      expect(loaded.trials[1].status, TrialStatus.incomplete);
      expect(loaded.trials[1].receiver!.frames.length, 2);
    });

    test('saving again replaces the record', () async {
      final s = TrialStore(dir);
      final t = receiverTrial('T-1', TrialStatus.running);
      await s.save(t);
      await s.save(t.copyWith(status: TrialStatus.cancelled));
      final loaded = await s.loadAll();
      expect(loaded.trials.single.status, TrialStatus.cancelled);
    });

    test('a trial left running by a dead process becomes incomplete', () async {
      final s = TrialStore(dir);
      await s.save(receiverTrial('T-1', TrialStatus.running));
      final loaded = await TrialStore(dir).loadAll();
      expect(loaded.recovered, 1);
      expect(loaded.trials.single.status, TrialStatus.incomplete);
      expect(loaded.trials.single.reason, TrialReason.interruptedByAppExit);
      // And the rewrite is persistent.
      final again = await TrialStore(dir).loadAll();
      expect(again.recovered, 0);
      expect(again.trials.single.status, TrialStatus.incomplete);
    });

    test('overlapping saves of one trial do not race', () async {
      final s = TrialStore(dir);
      final t = receiverTrial('T-1', TrialStatus.running);
      // The start-of-trial and end-of-trial writes can overlap in the app.
      await Future.wait([
        s.save(t),
        s.save(t.copyWith(status: TrialStatus.cancelled)),
      ]);
      final loaded = await s.loadAll();
      expect(loaded.trials.single.status, TrialStatus.cancelled);
    });

    test('a corrupt file is counted and does not hide the others', () async {
      final s = TrialStore(dir);
      await s.save(receiverTrial('T-1', TrialStatus.cancelled));
      File('${dir.path}/trials/broken.json').writeAsStringSync('{not json');
      final loaded = await s.loadAll();
      expect(loaded.trials.length, 1);
      expect(loaded.unreadable, 1);
    });

    test('annotations and results state persist', () async {
      final s = TrialStore(dir);
      expect((await s.loadAnnotations()).variant.isUnknown, isTrue);
      await s.saveAnnotations(
        Annotations(
          variant: Observed<String>.manual('8/128'),
          senderHardware: Observed<String>.manual('iPhone 13'),
        ),
      );
      final back = await TrialStore(dir).loadAnnotations();
      expect(back.variant.value, '8/128');
      expect(back.variant.source, ValueSource.manual);
      expect(back.region.isUnknown, isTrue);
      expect(back.senderHardware.value, 'iPhone 13');
    });
  });

  group('sharing', () {
    late FakeResultSharer sharer;
    late TrialStore store;
    late ResultsService service;
    final trials = [
      receiverTrial(
        'T-1',
        TrialStatus.completed,
        completionMs: 900,
        integrity: Integrity.ok,
      ),
      receiverTrial('T-2', TrialStatus.incomplete),
    ];

    ResultsService makeService() => ResultsService(
      store: store,
      exporter: ReportExporter(
        directory: Directory('${dir.path}/reports'),
        wallClock: () => DateTime.utc(2026, 1, 1, 0, 0, 0),
      ),
      sharer: sharer,
      wallClock: () => DateTime.utc(2026, 1, 2),
    );

    setUp(() {
      sharer = FakeResultSharer();
      store = TrialStore(dir);
      service = makeService();
    });

    Future<ShareRecord?> send({List<Trial>? t}) => service.sendResults(
      trials: t ?? trials,
      device: fakeDevice(),
      annotations: const Annotations(),
    );

    test(
      'a successful handoff is labelled as a handoff, not as delivery',
      () async {
        final r = (await send())!;
        expect(r.outcome.kind, ShareOutcomeKind.handedToShareSheet);
        expect(
          r.outcome.label,
          'Handed to the share sheet (delivery not confirmed)',
        );
        expect(
          r.outcome.label.toLowerCase(),
          isNot(contains('sent successfully')),
        );
        expect(sharer.shared.single.existsSync(), isTrue);
      },
    );

    test(
      'dismissing the share sheet keeps the report and allows a retry',
      () async {
        sharer.outcome = const ShareOutcome.dismissed();
        final first = (await send())!;
        expect(first.outcome.kind, ShareOutcomeKind.dismissed);
        final bundle = sharer.shared.single;
        expect(bundle.existsSync(), isTrue, reason: 'kept after cancel');
        expect(first.outcome.label, contains('kept'));

        sharer.outcome = const ShareOutcome.handedToShareSheet();
        final second = (await send())!;
        expect(second.outcome.kind, ShareOutcomeKind.handedToShareSheet);
        expect(
          sharer.shared.last.path,
          bundle.path,
          reason: 'same bundle reused',
        );
        expect(Directory('${dir.path}/reports').listSync().length, 1);
      },
    );

    test('a failing share keeps the report and records the error', () async {
      sharer.outcome = const ShareOutcome.failed('no share targets');
      final r = (await send())!;
      expect(r.outcome.kind, ShareOutcomeKind.failed);
      expect(r.outcome.label, contains('no share targets'));
      expect(File(r.bundlePath).existsSync(), isTrue);
      expect(service.lastExport!.file.existsSync(), isTrue);
    });

    test('new trials after an export make Send write a fresh bundle', () async {
      await send();
      final more = [...trials, receiverTrial('T-3', TrialStatus.cancelled)];
      await send(t: more);
      expect(sharer.shared.length, 2);
      expect(sharer.shared[0].path, isNot(sharer.shared[1].path));
      final b = readBundle(sharer.shared[1].readAsBytesSync());
      expect(b.report.trials.length, 3);
    });

    test(
      'export alone does not share, and its state survives a restart',
      () async {
        final e = await service.exportResults(
          trials: trials,
          device: fakeDevice(),
          annotations: const Annotations(),
        );
        expect(e, isNotNull);
        expect(sharer.shared, isEmpty);
        await send();

        final restarted = makeService();
        await restarted.init();
        expect(restarted.lastExport!.file.path, service.lastExport!.file.path);
        expect(
          restarted.lastShare!.outcome.kind,
          ShareOutcomeKind.handedToShareSheet,
        );
      },
    );

    test('an export failure is reported and nothing is shared', () async {
      // A file where the reports directory should be makes the write fail.
      File('${dir.path}/reports').writeAsStringSync('in the way');
      final r = await send();
      expect(r, isNull);
      expect(service.lastError, startsWith('Export failed'));
      expect(sharer.shared, isEmpty);
    });
  });

  group('SharePlusResultSharer maps the plugin result', () {
    const channel = MethodChannel('dev.fluttercommunity.plus/share');
    late File bundle;

    setUp(() {
      bundle = File('${dir.path}/b.zip')..writeAsBytesSync([1, 2, 3]);
    });
    tearDown(
      () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, null),
    );

    Future<ShareOutcome> run(Future<Object?> Function(MethodCall) handler) {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, handler);
      return const SharePlusResultSharer().share(bundle, subject: 's');
    }

    test('chosen target -> handed to the share sheet', () async {
      final o = await run((_) async => 'com.example.mail');
      expect(o.kind, ShareOutcomeKind.handedToShareSheet);
    });

    test('closed sheet -> dismissed', () async {
      expect((await run((_) async => '')).kind, ShareOutcomeKind.dismissed);
    });

    test('unknown result -> unavailable, not success', () async {
      final o = await run(
        (_) async => 'dev.fluttercommunity.plus/share/unavailable',
      );
      expect(o.kind, ShareOutcomeKind.unavailable);
      expect(o.label, contains('delivery not confirmed'));
    });

    test('platform error -> failed, bundle untouched', () async {
      final o = await run(
        (_) async => throw PlatformException(code: 'boom', message: 'nope'),
      );
      expect(o.kind, ShareOutcomeKind.failed);
      expect(o.error, contains('boom'));
      expect(bundle.existsSync(), isTrue);
    });
  });
}
